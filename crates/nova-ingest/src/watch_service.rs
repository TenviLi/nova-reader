//! Library Watch Service — bridges the filesystem watcher with the library scanner.
//!
//! When a library has `auto_scan: true`, this service watches its root directory
//! for file system events and triggers incremental re-scans on changes.
//!
//! Events are debounced (500ms window) and categorized:
//! - Created: new book file → enqueue ingest task
//! - Modified: file changed → re-hash, re-ingest if content changed
//! - Removed: file deleted → mark book as missing/archived

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn};

use crate::watcher::{FileWatcher, FileEvent};
use crate::scanner::LibraryScanner;

/// Manages multiple file watchers, one per library.
pub struct LibraryWatchService {
    /// Map of library_id → active watcher handles
    watchers: Arc<RwLock<HashMap<String, WatchHandle>>>,
}

struct WatchHandle {
    library_id: String,
    root_path: PathBuf,
    /// Channel to signal shutdown
    shutdown_tx: mpsc::Sender<()>,
}

/// Events emitted by the library watch service for the worker to process.
#[derive(Debug, Clone)]
pub enum LibraryEvent {
    /// A new book file appeared in a library.
    BookAdded {
        library_id: String,
        file_path: PathBuf,
    },
    /// A book file was modified (content may have changed).
    BookModified {
        library_id: String,
        file_path: PathBuf,
    },
    /// A book file was removed from the filesystem.
    BookRemoved {
        library_id: String,
        file_path: PathBuf,
    },
}

impl LibraryWatchService {
    pub fn new() -> Self {
        Self {
            watchers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start watching a library directory.
    /// Returns a receiver channel for library events.
    pub async fn watch_library(
        &self,
        library_id: String,
        root_path: PathBuf,
        exclude_patterns: Vec<String>,
    ) -> nova_core::Result<mpsc::Receiver<LibraryEvent>> {
        let (event_tx, event_rx) = mpsc::channel(256);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        let existing = self.watchers.write().await.remove(&library_id);
        if let Some(handle) = existing {
            warn!(
                library = %handle.library_id,
                path = %handle.root_path.display(),
                "Replacing existing library watcher"
            );
            let _ = handle.shutdown_tx.send(()).await;
        }

        let scanner = LibraryScanner::new(
            vec!["txt".into(), "epub".into(), "pdf".into(), "docx".into(), "doc".into(), "md".into(), "html".into()],
            exclude_patterns,
        );

        // Start the underlying file watcher
        let watcher = FileWatcher::new(root_path.clone(), 500);
        let mut file_events = watcher.start().await?;

        let lib_id = library_id.clone();
        let lib_root = root_path.clone();

        // Spawn event processing task
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(file_event) = file_events.recv() => {
                        let lib_event = match file_event {
                            FileEvent::Created(path) => {
                                // Check if the file should be excluded
                                if should_exclude_path(&scanner, &lib_root, &path) {
                                    continue;
                                }
                                info!(path = %path.display(), library = %lib_id, "New book file detected");
                                LibraryEvent::BookAdded {
                                    library_id: lib_id.clone(),
                                    file_path: path,
                                }
                            }
                            FileEvent::Modified(path) => {
                                if should_exclude_path(&scanner, &lib_root, &path) {
                                    continue;
                                }
                                info!(path = %path.display(), library = %lib_id, "Book file modified");
                                LibraryEvent::BookModified {
                                    library_id: lib_id.clone(),
                                    file_path: path,
                                }
                            }
                            FileEvent::Removed(path) => {
                                if should_exclude_path(&scanner, &lib_root, &path) {
                                    continue;
                                }
                                info!(path = %path.display(), library = %lib_id, "Book file removed");
                                LibraryEvent::BookRemoved {
                                    library_id: lib_id.clone(),
                                    file_path: path,
                                }
                            }
                        };
                        if event_tx.send(lib_event).await.is_err() {
                            break; // Receiver dropped
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!(library = %lib_id, "Library watcher shutting down");
                        break;
                    }
                }
            }
        });

        // Store the handle
        let handle = WatchHandle {
            library_id: library_id.clone(),
            root_path,
            shutdown_tx,
        };
        self.watchers.write().await.insert(library_id, handle);

        Ok(event_rx)
    }

    /// Stop watching a library.
    pub async fn unwatch_library(&self, library_id: &str) {
        if let Some(handle) = self.watchers.write().await.remove(library_id) {
            let _ = handle.shutdown_tx.send(()).await;
            info!(
                library = %handle.library_id,
                path = %handle.root_path.display(),
                "Library watcher stopped"
            );
        }
    }

    /// Stop all watchers (for graceful shutdown).
    pub async fn shutdown_all(&self) {
        let mut watchers = self.watchers.write().await;
        for (_, handle) in watchers.drain() {
            let _ = handle.shutdown_tx.send(()).await;
            info!(
                library = %handle.library_id,
                path = %handle.root_path.display(),
                "Watcher shut down"
            );
        }
    }

    /// Get list of currently watched library IDs.
    pub async fn watched_libraries(&self) -> Vec<String> {
        self.watchers
            .read()
            .await
            .values()
            .map(|handle| handle.library_id.clone())
            .collect()
    }
}

fn should_exclude_path(scanner: &LibraryScanner, root: &Path, path: &Path) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);

    if relative.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        scanner.should_exclude(name.as_ref())
    }) {
        return true;
    }

    let relative_name = relative.to_string_lossy();
    scanner.should_exclude(relative_name.as_ref())
}
