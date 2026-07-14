//! # Nova Ingest
//!
//! Handles file system monitoring, document parsing, and chapter extraction.
//! Implements debounced file watching (500ms window) and multi-format parsing.
//! Includes Komga/Kavita-style library scanning for series discovery.

pub mod chapter_splitter;
pub mod cleaner;
pub mod cover;
pub mod dedup;
pub mod dedup_benchmark;
pub mod dedup_evaluation;
pub mod error;
pub mod hasher;
pub mod parser;
pub mod scanner;
pub mod watch_service;
pub mod watcher;

pub use chapter_splitter::ChapterSplitter;
pub use cover::extract_cover;
pub use parser::DocumentParser;
pub use scanner::LibraryScanner;
pub use watch_service::LibraryWatchService;
pub use watcher::FileWatcher;

#[cfg(test)]
mod tests;
