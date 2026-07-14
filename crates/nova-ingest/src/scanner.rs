//! Library Scanner — Komga/Kavita-style filesystem traversal.
//!
//! Scans a library root directory and discovers:
//! - Series (subfolders containing book files)
//! - Books (supported files within series folders)
//! - Metadata sidecars (series.json, metadata.json)
//!
//! Detection rules:
//! 1. Top-level subfolders = series
//! 2. Files directly in library root = standalone "series" (one book = one series)
//! 3. Nested subfolders are flattened into the closest parent series
//! 4. Volume ordering extracted from filename patterns

use std::path::{Path, PathBuf};
use std::time::Instant;

use tokio::fs;
use tracing::{debug, info, warn};

use nova_core::domain::book::BookFormat;

use crate::error::ScannerResult;

/// Result of scanning a library directory.
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub library_path: PathBuf,
    pub series: Vec<DiscoveredSeries>,
    pub orphan_files: Vec<DiscoveredBook>,
    pub duration_ms: u64,
    pub total_files: usize,
    pub total_bytes: u64,
    pub errors: Vec<ScanError>,
}

/// A series discovered from a folder structure.
#[derive(Debug, Clone)]
pub struct DiscoveredSeries {
    pub folder_path: PathBuf,
    pub name: String,
    pub sort_name: String,
    pub books: Vec<DiscoveredBook>,
    pub metadata: Option<SeriesSidecar>,
    pub total_bytes: u64,
}

/// A book file discovered during scanning.
#[derive(Debug, Clone)]
pub struct DiscoveredBook {
    pub file_path: PathBuf,
    pub filename: String,
    pub format: BookFormat,
    pub file_size: u64,
    pub modified_at: std::time::SystemTime,
    /// Extracted volume/chapter number from filename pattern
    pub sort_number: Option<f64>,
    /// SHA-256 file hash for deduplication (computed lazily)
    pub file_hash: Option<String>,
}

/// Metadata sidecar (series.json or metadata.json).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SeriesSidecar {
    pub name: Option<String>,
    pub sort_name: Option<String>,
    pub author: Option<String>,
    pub artist: Option<String>,
    pub summary: Option<String>,
    pub status: Option<String>,
    pub genres: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub language: Option<String>,
    pub year: Option<i32>,
    pub age_rating: Option<String>,
    pub publisher: Option<String>,
    pub alternate_titles: Option<Vec<String>>,
    pub external_links: Option<serde_json::Value>,
}

/// Error encountered during scanning (non-fatal).
#[derive(Debug, Clone)]
pub struct ScanError {
    pub path: PathBuf,
    pub message: String,
}

/// Library scanner configuration.
pub struct LibraryScanner {
    /// Supported file extensions (lowercase, no dot).
    pub extensions: Vec<String>,
    /// Glob patterns to exclude.
    pub exclude_patterns: Vec<String>,
    /// Whether to compute file hashes during scan (slower but enables dedup).
    pub compute_hashes: bool,
}

impl LibraryScanner {
    pub fn new(extensions: Vec<String>, exclude_patterns: Vec<String>) -> Self {
        Self {
            extensions,
            exclude_patterns,
            compute_hashes: false,
        }
    }

    /// Create scanner with hash computation enabled.
    pub fn with_hashing(mut self) -> Self {
        self.compute_hashes = true;
        self
    }

    /// Default scanner for all supported formats with NAS-friendly excludes.
    pub fn default() -> Self {
        Self {
            extensions: vec![
                "txt".into(),
                "epub".into(),
                "pdf".into(),
                "docx".into(),
                "doc".into(),
                "md".into(),
                "html".into(),
            ],
            exclude_patterns: vec![
                ".*".into(), // Hidden files/folders (covers .DS_Store, .Trash, etc.)
                "Thumbs.db".into(),
                "__MACOSX".into(),
                // Synology NAS
                "#recycle".into(),
                "@eaDir".into(),
                "@tmp".into(),
                // QNAP NAS
                "@Recycle".into(),
                ".@__thumb".into(),
                // Generic NAS/Windows
                "$RECYCLE.BIN".into(),
                "System Volume Information".into(),
                // Docker-specific
                ".docker_temp".into(),
                // Common temp patterns
                "*.tmp".into(),
                "*.partial".into(),
                "~$*".into(),          // Office lock files
                "*.crdownload".into(), // Chrome partial downloads
                "*.part".into(),       // Firefox partial downloads
            ],
            compute_hashes: false,
        }
    }

    /// Scan a library root directory and discover all series and books.
    pub async fn scan(&self, root: &Path) -> ScannerResult<ScanResult> {
        let start = Instant::now();
        let mut series_list = Vec::new();
        let mut orphan_files = Vec::new();
        let mut errors = Vec::new();
        let mut total_files = 0usize;
        let mut total_bytes = 0u64;

        info!(path = %root.display(), "Starting library scan");

        // Read top-level entries
        let mut entries = fs::read_dir(root).await?;
        let mut top_level_dirs = Vec::new();
        let mut top_level_files = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip excluded patterns
            if self.should_exclude(&name) {
                debug!(path = %path.display(), "Excluded");
                continue;
            }

            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                top_level_dirs.push(path);
            } else if file_type.is_file() {
                top_level_files.push(path);
            }
        }

        // Process each top-level directory as a series
        for dir_path in top_level_dirs {
            match self.scan_series_folder(&dir_path).await {
                Ok(series) => {
                    total_files += series.books.len();
                    total_bytes += series.total_bytes;
                    series_list.push(series);
                }
                Err(e) => {
                    errors.push(ScanError {
                        path: dir_path,
                        message: e.to_string(),
                    });
                }
            }
        }

        // Files directly in root become standalone "orphans" (grouped later)
        for file_path in top_level_files {
            if let Some(book) = self.try_parse_book_file(&file_path).await? {
                total_files += 1;
                total_bytes += book.file_size;
                orphan_files.push(book);
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        info!(
            series = series_list.len(),
            files = total_files,
            bytes = total_bytes,
            duration_ms,
            "Library scan complete"
        );

        Ok(ScanResult {
            library_path: root.to_path_buf(),
            series: series_list,
            orphan_files,
            duration_ms,
            total_files,
            total_bytes,
            errors,
        })
    }

    /// Scan a single series folder (recursively finds all book files).
    async fn scan_series_folder(&self, folder: &Path) -> ScannerResult<DiscoveredSeries> {
        let folder_name = folder
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let mut books = Vec::new();
        let mut metadata = None;
        let mut total_bytes = 0u64;

        // Recursively find all files in this series folder
        self.collect_files_recursive(folder, &mut books, &mut metadata)
            .await?;

        for book in &books {
            total_bytes += book.file_size;
        }

        // Sort books by extracted sort number, then by filename
        books.sort_by(|a, b| match (a.sort_number, b.sort_number) {
            (Some(na), Some(nb)) => na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.filename.cmp(&b.filename),
        });

        let name = metadata
            .as_ref()
            .and_then(|m| m.name.clone())
            .unwrap_or_else(|| folder_name.clone());

        let sort_name = metadata
            .as_ref()
            .and_then(|m| m.sort_name.clone())
            .unwrap_or_else(|| generate_sort_name(&name));

        Ok(DiscoveredSeries {
            folder_path: folder.to_path_buf(),
            name,
            sort_name,
            books,
            metadata,
            total_bytes,
        })
    }

    /// Recursively collect book files from a directory tree.
    async fn collect_files_recursive(
        &self,
        dir: &Path,
        books: &mut Vec<DiscoveredBook>,
        metadata: &mut Option<SeriesSidecar>,
    ) -> ScannerResult<()> {
        let mut entries = fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if self.should_exclude(&name) {
                continue;
            }

            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                // Recurse into subdirectories (nested series structure)
                Box::pin(self.collect_files_recursive(&path, books, metadata)).await?;
            } else if file_type.is_file() {
                // Check for metadata sidecar
                if metadata.is_none() && is_metadata_file(&name) {
                    if let Ok(content) = fs::read_to_string(&path).await {
                        match serde_json::from_str::<SeriesSidecar>(&content) {
                            Ok(sidecar) => *metadata = Some(sidecar),
                            Err(e) => {
                                warn!(path = %path.display(), error = %e, "Invalid metadata sidecar")
                            }
                        }
                    }
                }

                // Check if it's a supported book file
                if let Some(book) = self.try_parse_book_file(&path).await? {
                    books.push(book);
                }
            }
        }

        Ok(())
    }

    /// Try to parse a file path into a DiscoveredBook.
    async fn try_parse_book_file(&self, path: &Path) -> ScannerResult<Option<DiscoveredBook>> {
        let Some(ext) = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_lowercase)
        else {
            return Ok(None);
        };
        if !self.extensions.contains(&ext) {
            return Ok(None);
        }

        let Some(format) = BookFormat::from_extension(&ext) else {
            return Ok(None);
        };
        let Ok(metadata) = fs::metadata(path).await else {
            return Ok(None);
        };
        let Some(filename) = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
        else {
            return Ok(None);
        };
        let sort_number = extract_number_from_filename(&filename)?;

        // Compute file hash if enabled
        let file_hash = if self.compute_hashes {
            match crate::hasher::hash_file(path).await {
                Ok(hash) => Some(hash),
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "Failed to hash file");
                    None
                }
            }
        } else {
            None
        };

        Ok(Some(DiscoveredBook {
            file_path: path.to_path_buf(),
            filename,
            format,
            file_size: metadata.len(),
            modified_at: metadata
                .modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            sort_number,
            file_hash,
        }))
    }

    /// Check if a filename/folder should be excluded.
    pub fn should_exclude(&self, name: &str) -> bool {
        // Always exclude hidden files (starting with .)
        if name.starts_with('.') {
            return true;
        }
        // Check explicit exclude patterns
        for pattern in &self.exclude_patterns {
            if self.matches_glob(name, pattern) {
                return true;
            }
        }
        false
    }

    /// Glob pattern matching supporting *, ?, **, and case-insensitive comparison.
    fn matches_glob(&self, name: &str, pattern: &str) -> bool {
        // Exact match (case-insensitive for NAS trash dirs)
        if name.eq_ignore_ascii_case(pattern) {
            return true;
        }
        // Use glob-match crate for proper glob matching (case-insensitive)
        glob_match::glob_match(&pattern.to_ascii_lowercase(), &name.to_ascii_lowercase())
    }
}

/// Check if a filename is a metadata sidecar.
fn is_metadata_file(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "series.json" | "metadata.json" | "comicinfo.json"
    )
}

/// Extract a number from common filename patterns.
/// Examples: "v01.txt" → 1.0, "Chapter 003.epub" → 3.0, "Book 1.5 - Side Story.txt" → 1.5
fn extract_number_from_filename(filename: &str) -> Result<Option<f64>, regex::Error> {
    use regex::Regex;
    use std::sync::LazyLock;

    // Remove extension
    let stem = filename
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(filename);

    static PATTERNS: LazyLock<Result<[Regex; 6], regex::Error>> = LazyLock::new(|| {
        Ok([
            Regex::new(r"(?i)(?:v|vol\.?\s*)(\d+(?:\.\d+)?)")?,
            Regex::new(r"第(\d+(?:\.\d+)?)(?:卷|章|册|部|集)")?,
            Regex::new(r"(?i)(?:ch(?:apter)?\.?\s*)(\d+(?:\.\d+)?)")?,
            Regex::new(r"(?i)(?:book\s+)?(\d+(?:\.\d+)?)\s*[-–—]")?,
            Regex::new(r"[-–—\s](\d+(?:\.\d+)?)\s*$")?,
            Regex::new(r"^(\d+(?:\.\d+)?)\s*[-–—.]")?,
        ])
    });

    let patterns = PATTERNS.as_ref().map_err(Clone::clone)?;
    for pattern in patterns {
        if let Some(caps) = pattern.captures(stem) {
            if let Some(m) = caps.get(1) {
                if let Ok(n) = m.as_str().parse::<f64>() {
                    return Ok(Some(n));
                }
            }
        }
    }

    Ok(None)
}

/// Generate a sort-friendly name (strips articles, normalizes).
fn generate_sort_name(name: &str) -> String {
    let name = name.trim();
    // Strip common articles for English sorting
    let prefixes = ["the ", "a ", "an "];
    let lower = name.to_lowercase();
    for prefix in prefixes {
        if lower.starts_with(prefix) {
            return name[prefix.len()..].to_string();
        }
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_number() -> Result<(), regex::Error> {
        assert_eq!(extract_number_from_filename("v01.txt")?, Some(1.0));
        assert_eq!(extract_number_from_filename("Vol.2 Title.epub")?, Some(2.0));
        assert_eq!(extract_number_from_filename("第3卷.txt")?, Some(3.0));
        assert_eq!(extract_number_from_filename("Chapter 005.epub")?, Some(5.0));
        assert_eq!(
            extract_number_from_filename("Book 1.5 - Side.txt")?,
            Some(1.5)
        );
        assert_eq!(
            extract_number_from_filename("Side Story - 07.txt")?,
            Some(7.0)
        );
        assert_eq!(
            extract_number_from_filename("001 - Opening.txt")?,
            Some(1.0)
        );
        assert_eq!(extract_number_from_filename("random.txt")?, None);
        Ok(())
    }

    #[tokio::test]
    async fn scan_reports_filename_sort_numbers_through_its_public_result_boundary(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "nova-scanner-fallible-patterns-{}",
            uuid::Uuid::now_v7()
        ));
        let series_dir = root.join("Series");
        fs::create_dir_all(&series_dir).await?;
        fs::write(series_dir.join("Chapter 10.txt"), "ten").await?;
        fs::write(series_dir.join("v02.txt"), "two").await?;

        let result = LibraryScanner::default().scan(&root).await;
        fs::remove_dir_all(&root).await?;
        let result = result?;

        assert_eq!(result.series.len(), 1);
        let sort_numbers: Vec<Option<f64>> = result.series[0]
            .books
            .iter()
            .map(|book| book.sort_number)
            .collect();
        assert_eq!(sort_numbers, vec![Some(2.0), Some(10.0)]);
        Ok(())
    }

    #[test]
    fn test_sort_name() {
        assert_eq!(
            generate_sort_name("The Lord of the Rings"),
            "Lord of the Rings"
        );
        assert_eq!(generate_sort_name("斗破苍穹"), "斗破苍穹");
        assert_eq!(generate_sort_name("A Song of Ice"), "Song of Ice");
    }

    #[test]
    fn test_metadata_file_detection() {
        assert!(is_metadata_file("series.json"));
        assert!(is_metadata_file("metadata.json"));
        assert!(is_metadata_file("SERIES.JSON"));
        assert!(!is_metadata_file("book.json"));
        assert!(!is_metadata_file("chapter.txt"));
    }

    #[test]
    fn test_should_exclude_nas_patterns() {
        let scanner = LibraryScanner::default();

        // Synology
        assert!(scanner.should_exclude("#recycle"));
        assert!(scanner.should_exclude("@eaDir"));
        assert!(scanner.should_exclude("@tmp"));

        // QNAP
        assert!(scanner.should_exclude("@Recycle"));

        // Windows
        assert!(scanner.should_exclude("$RECYCLE.BIN"));
        assert!(scanner.should_exclude("System Volume Information"));

        // Hidden files
        assert!(scanner.should_exclude(".DS_Store"));
        assert!(scanner.should_exclude(".Trash-1000"));

        // Temp files
        assert!(scanner.should_exclude("document.tmp"));
        assert!(scanner.should_exclude("file.partial"));
        assert!(scanner.should_exclude("~$word.docx"));
        assert!(scanner.should_exclude("download.crdownload"));

        // Normal files should NOT be excluded
        assert!(!scanner.should_exclude("novel.txt"));
        assert!(!scanner.should_exclude("第1卷.epub"));
        assert!(!scanner.should_exclude("My Series"));
    }

    #[test]
    fn test_glob_matching() {
        let scanner = LibraryScanner::default();

        assert!(scanner.matches_glob("file.tmp", "*.tmp"));
        assert!(scanner.matches_glob("long name.tmp", "*.tmp"));
        assert!(!scanner.matches_glob("file.txt", "*.tmp"));
        assert!(scanner.matches_glob("~$word.docx", "~$*"));
        assert!(scanner.matches_glob("test", "?est"));
        assert!(!scanner.matches_glob("rest!", "?est"));
        assert!(scanner.matches_glob("download.part", "*.part"));
    }
}
