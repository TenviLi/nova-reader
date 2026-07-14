use std::path::{Path, PathBuf};
use std::time::Duration;

use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use tokio::sync::mpsc;
use tracing::{error, info};

use nova_core::domain::book::BookFormat;

/// Events emitted by the file watcher.
#[derive(Debug, Clone)]
pub enum FileEvent {
    /// A new file was detected in the inbox.
    Created(PathBuf),
    /// A file was modified.
    Modified(PathBuf),
    /// A file was removed.
    Removed(PathBuf),
}

/// Debounced file system watcher for the inbox directory.
/// Collapses rapid filesystem events into single notifications
/// using a 500ms debounce window.
pub struct FileWatcher {
    inbox_path: PathBuf,
    debounce_ms: u64,
}

impl FileWatcher {
    pub fn new(inbox_path: impl Into<PathBuf>, debounce_ms: u64) -> Self {
        Self {
            inbox_path: inbox_path.into(),
            debounce_ms,
        }
    }

    /// Start watching the inbox directory.
    /// Returns a channel receiver that emits debounced file events.
    pub async fn start(&self) -> nova_core::Result<mpsc::Receiver<FileEvent>> {
        let (tx, rx) = mpsc::channel(256);
        let inbox_path = self.inbox_path.clone();
        let debounce_duration = Duration::from_millis(self.debounce_ms);

        // Ensure inbox directory exists
        tokio::fs::create_dir_all(&inbox_path).await?;

        info!(path = %inbox_path.display(), "Starting file watcher");

        // Spawn the watcher in a blocking thread (notify uses OS APIs)
        let tx_clone = tx.clone();
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();

            let mut debouncer = new_debouncer(
                debounce_duration,
                move |events: Result<Vec<notify_debouncer_mini::DebouncedEvent>, _>| {
                    match events {
                        Ok(events) => {
                            for event in events {
                                let path = event.path.clone();

                                // Only process supported file types
                                if !is_supported_file(&path) {
                                    continue;
                                }

                                let file_event = match event.kind {
                                    DebouncedEventKind::Any => {
                                        if path.exists() {
                                            FileEvent::Created(path)
                                        } else {
                                            FileEvent::Removed(path)
                                        }
                                    }
                                    DebouncedEventKind::AnyContinuous => {
                                        FileEvent::Modified(path)
                                    }
                                    _ => continue,
                                };

                                let tx = tx_clone.clone();
                                rt.spawn(async move {
                                    if let Err(e) = tx.send(file_event).await {
                                        error!("Failed to send file event: {}", e);
                                    }
                                });
                            }
                        }
                        Err(e) => {
                            error!("File watcher error: {:?}", e);
                        }
                    }
                },
            )
            .map_err(|e| nova_core::Error::Internal(format!("Failed to create debouncer: {}", e)));

            match debouncer {
                Ok(ref mut debouncer) => {
                    if let Err(e) = debouncer
                        .watcher()
                        .watch(&inbox_path, notify::RecursiveMode::Recursive)
                    {
                        error!("Failed to watch directory: {}", e);
                    } else {
                        info!("File watcher active on: {}", inbox_path.display());
                        // Keep the thread alive
                        loop {
                            std::thread::sleep(Duration::from_secs(1));
                        }
                    }
                }
                Err(e) => {
                    error!("Debouncer creation failed: {:?}", e);
                }
            }
        });

        Ok(rx)
    }
}

/// Check if a file has a supported extension.
fn is_supported_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(BookFormat::from_extension)
        .is_some()
}
