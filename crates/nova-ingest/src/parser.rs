use std::path::Path;

use kreuzberg::{extract_file, ExtractionConfig};
use nova_core::domain::book::BookFormat;
use tracing::info;

/// Multi-format document parser powered by Kreuzberg.
/// Supports 91+ formats: PDF, EPUB, DOCX, HTML, TXT, Markdown, etc.
pub struct DocumentParser;

/// Parsed document result.
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    /// Extracted title (from metadata or filename)
    pub title: String,
    /// Detected author
    pub author: Option<String>,
    /// Full text content
    pub content: String,
    /// Detected language
    pub language: nova_core::domain::book::Language,
    /// File format
    pub format: BookFormat,
    /// Word count
    pub word_count: i64,
    /// Raw metadata from Kreuzberg extraction
    pub metadata: DocumentMetadata,
}

/// Additional metadata extracted during parsing.
#[derive(Debug, Clone, Default)]
pub struct DocumentMetadata {
    pub page_count: Option<u32>,
    pub creation_date: Option<String>,
    pub subject: Option<String>,
    pub keywords: Vec<String>,
}

impl DocumentParser {
    /// Parse a document file using Kreuzberg's unified extraction API.
    /// Handles PDF, EPUB, DOCX, HTML, TXT, Markdown, and 85+ other formats.
    pub async fn parse(path: &Path) -> nova_core::Result<ParsedDocument> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let format = BookFormat::from_extension(ext)
            .ok_or_else(|| nova_core::Error::Parse(format!("Unsupported format: {}", ext)))?;

        info!(path = %path.display(), format = ?format, "Parsing document with Kreuzberg");

        let config = ExtractionConfig::default();
        let result = extract_file(path, None, &config)
            .await
            .map_err(|e| nova_core::Error::Parse(format!("Kreuzberg extraction failed: {e}")))?;

        let content = result.content;
        let title = extract_title_from_path(path);
        let language = detect_language(&content);
        let word_count = count_words(&content);

        Ok(ParsedDocument {
            title,
            author: None, // TODO: extract from Kreuzberg metadata when available
            content,
            language,
            format,
            word_count,
            metadata: DocumentMetadata::default(),
        })
    }

    /// Parse from raw bytes with a known MIME type.
    pub async fn parse_bytes(data: &[u8], mime_type: &str, filename: &str) -> nova_core::Result<ParsedDocument> {
        let config = ExtractionConfig::default();
        let result = kreuzberg::extract_bytes(data, mime_type, &config)
            .await
            .map_err(|e| nova_core::Error::Parse(format!("Kreuzberg extraction failed: {e}")))?;

        let content = result.content;
        let ext = Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("txt");
        let format = BookFormat::from_extension(ext).unwrap_or(BookFormat::Txt);
        let language = detect_language(&content);
        let word_count = count_words(&content);

        Ok(ParsedDocument {
            title: Path::new(filename)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled")
                .to_string(),
            author: None,
            content,
            language,
            format,
            word_count,
            metadata: DocumentMetadata::default(),
        })
    }

    /// Batch parse multiple files concurrently using Kreuzberg's batch API.
    pub async fn parse_batch(paths: &[&Path]) -> Vec<nova_core::Result<ParsedDocument>> {
        let config = ExtractionConfig::default();
        let inputs: Vec<(std::path::PathBuf, Option<kreuzberg::FileExtractionConfig>)> = paths
            .iter()
            .map(|p| (p.to_path_buf(), None))
            .collect();

        match kreuzberg::batch_extract_file(inputs, &config).await {
            Ok(results) => {
                results
                    .into_iter()
                    .zip(paths.iter())
                    .map(|(result, path)| {
                        let content = result.content;
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                        let format = BookFormat::from_extension(ext).unwrap_or(BookFormat::Txt);
                        let language = detect_language(&content);
                        let word_count = count_words(&content);

                        Ok(ParsedDocument {
                            title: extract_title_from_path(path),
                            author: None,
                            content,
                            language,
                            format,
                            word_count,
                            metadata: DocumentMetadata::default(),
                        })
                    })
                    .collect()
            }
            Err(e) => {
                vec![Err(nova_core::Error::Parse(format!("Batch extraction failed: {e}")))]
            }
        }
    }
}

fn extract_title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

/// Language detection based on character frequency.
/// Samples from beginning, middle, and end of text for better accuracy.
fn detect_language(text: &str) -> nova_core::domain::book::Language {
    let len = text.len();
    if len == 0 {
        return nova_core::domain::book::Language::Unknown;
    }

    // Sample from multiple positions for reliability (beginning, middle, end)
    let sample_size = 2000;
    let samples: Vec<&str> = if len <= sample_size * 2 {
        vec![&text[..len.min(sample_size * 2)]]
    } else {
        let mid = len / 2;
        let end_start = len.saturating_sub(sample_size);
        vec![
            &text[..sample_size.min(len)],
            &text[mid.saturating_sub(sample_size / 2)..mid.saturating_add(sample_size / 2).min(len)],
            &text[end_start..],
        ]
    };

    let mut cjk_count: usize = 0;
    let mut jp_count: usize = 0;
    let mut kr_count: usize = 0;
    let mut total_chars: usize = 0;

    for sample in &samples {
        for c in sample.chars() {
            total_chars += 1;
            let cp = c as u32;
            match cp {
                0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x2E80..=0x2EFF | 0x3000..=0x303F => {
                    cjk_count += 1;
                }
                0x3040..=0x309F | 0x30A0..=0x30FF => {
                    jp_count += 1;
                }
                0xAC00..=0xD7AF => {
                    kr_count += 1;
                }
                _ => {}
            }
        }
    }

    if total_chars == 0 {
        return nova_core::domain::book::Language::Unknown;
    }

    let total = total_chars as f64;
    let jp_ratio = jp_count as f64 / total;
    let kr_ratio = kr_count as f64 / total;
    let cjk_ratio = cjk_count as f64 / total;

    if jp_ratio > 0.05 {
        nova_core::domain::book::Language::Japanese
    } else if kr_ratio > 0.05 {
        nova_core::domain::book::Language::Korean
    } else if cjk_ratio > 0.1 {
        nova_core::domain::book::Language::Chinese
    } else {
        nova_core::domain::book::Language::English
    }
}

/// Count words in text (handles CJK where each character ≈ a word).
fn count_words(text: &str) -> i64 {
    use unicode_segmentation::UnicodeSegmentation;

    let mut count: i64 = 0;
    for word in text.unicode_words() {
        if word.chars().any(|c| (c as u32) >= 0x4E00) {
            count += word.chars().count() as i64;
        } else {
            count += 1;
        }
    }
    count
}
