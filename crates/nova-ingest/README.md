# nova-ingest

文件摄取引擎 — Komga/Kavita 风格的书库扫描与文档解析。

## 职责

- **书库扫描器** (`LibraryScanner`): 递归发现系列/图书, NAS 垃圾排除, glob 过滤
- **文档解析** (`DocumentParser`): 91+ 格式支持 (via Kreuzberg)
- **章节分割** (`ChapterSplitter`): 中英文章节标记自动识别
- **封面提取** (`extract_cover`): EPUB/PDF 封面图片提取
- **文件监听** (`FileWatcher` + `LibraryWatchService`): 实时文件变更检测
- **文件哈希** (`hasher`): SHA-256 文件去重

## 关键依赖

| 库 | 用途 |
|-----|------|
| `kreuzberg` | 文档解析 (PDF, EPUB, DOCX, HTML, 图片 OCR) |
| `walkdir` | 高效目录遍历 |
| `notify` | 跨平台文件系统监听 |
| `glob-match` | Glob 模式匹配 (替代手写实现) |
| `sha2` + `hex` | SHA-256 文件哈希 |
| `regex` | 章节/卷号提取 |

## NAS 支持

自动排除各 NAS 系统垃圾目录:
- **Synology**: `#recycle`, `@eaDir`, `@tmp`
- **QNAP**: `@Recycle`, `.@__thumb`
- **Windows**: `$RECYCLE.BIN`, `System Volume Information`
- **macOS**: `.Spotlight-V100`, `.fseventsd`
- **Docker**: `.docker-temp`

## 使用示例

```rust
use nova_ingest::LibraryScanner;

let scanner = LibraryScanner::new()
    .with_extensions(vec!["epub", "txt", "pdf"])
    .with_exclude_patterns(vec!["*.tmp", "#recycle"])
    .with_hashing(true);

let result = scanner.scan("/mnt/nas/books").await?;
println!("Found {} series, {} books", result.series.len(), result.total_files);
```
