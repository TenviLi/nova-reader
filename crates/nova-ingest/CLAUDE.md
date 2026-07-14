# nova-ingest

File system monitoring, multi-format document parsing (91+ formats via Kreuzberg 4.9), intelligent chapter extraction, series discovery, and debounced library scanning.

## Architecture

```
src/
├── lib.rs              — Re-exports
├── parser.rs           — DocumentParser (Kreuzberg wrapper)
├── chapter_splitter.rs — ChapterSplitter (intelligent splitting)
├── scanner.rs          — LibraryScanner (Komga/Kavita-style discovery)
├── watcher.rs          — FileWatcher (OS-level file events)
├── watch_service.rs    — LibraryWatchService (500ms debounce)
├── cover.rs            — Cover image extraction from EPUB/PDF
├── hasher.rs           — SHA256 file hashing for dedup
└── cleaner.rs          — Text cleaning (watermarks, ads removal)
```

## Supported Formats

Via Kreuzberg features `pdf`, `office`, `html`:
- **Books**: EPUB, PDF, MOBI, FB2, AZW3
- **Documents**: DOCX, DOC, ODT, RTF, TXT, Markdown, HTML
- **Archives**: ZIP (containing supported formats)

## Key Types

```rust
pub struct DocumentParser;
impl DocumentParser {
    pub async fn parse(path: &Path) -> Result<ParsedDocument>;
    pub async fn parse_bytes(data: &[u8], mime: &str, filename: &str) -> Result<ParsedDocument>;
}

pub struct ChapterSplitter;
impl ChapterSplitter {
    pub fn split(text: &str) -> Vec<Chapter>;  // Detects chapter markers, splits by size
}

pub struct LibraryScanner;
impl LibraryScanner {
    pub async fn scan(root: &Path) -> Result<Vec<SeriesEntry>>;  // Author/Series/Book structure
}
```

## Key Patterns

- **Kreuzberg delegation**: All format detection/extraction delegated to Kreuzberg
- **Chapter detection heuristics**: Regex for `第X章`, `Chapter X`, `## `, `---` markers
- **Size-based fallback**: If no markers found, split at 5000-word boundaries
- **Debounced watching**: 500ms coalesce window prevents duplicate events
- **Dedup via hash**: SHA256 prevents re-ingesting identical files
- **Series discovery**: Folder structure `/Library/Author/Series/Book.epub`

## Dependencies

- **Internal**: nova-core
- **External**: kreuzberg, notify, notify-debouncer-mini, walkdir, regex, sha2, tokio

## Build & Test

```bash
cargo build -p nova-ingest
cargo test -p nova-ingest
```
