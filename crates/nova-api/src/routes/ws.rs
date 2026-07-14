//! WebSocket handler for real-time reading progress synchronization.
//! Enables multi-device reading position sync without page refresh.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/progress", get(ws_progress_handler))
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum WsMessage {
    /// Client sends position update
    #[serde(rename = "position_update")]
    PositionUpdate {
        book_id: String,
        chapter_index: usize,
        scroll_position: f64,
        timestamp: u64,
    },
    /// Server broadcasts position to other devices
    #[serde(rename = "position_sync")]
    PositionSync {
        book_id: String,
        chapter_index: usize,
        scroll_position: f64,
        device: String,
        timestamp: u64,
    },
    /// Heartbeat
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
}

async fn ws_progress_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to broadcast channel for multi-device sync
    let mut rx = state.progress_broadcast.subscribe();

    // Spawn task to forward broadcasts to this client
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            let text = serde_json::to_string(&msg).unwrap_or_default();
            if sender.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages from this client
    let tx = state.progress_broadcast.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(message)) = receiver.next().await {
            match message {
                Message::Text(text) => {
                    if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text) {
                        match ws_msg {
                            WsMessage::PositionUpdate { book_id, chapter_index, scroll_position, timestamp } => {
                                // Broadcast to other connected devices
                                let sync_msg = WsMessage::PositionSync {
                                    book_id,
                                    chapter_index,
                                    scroll_position,
                                    device: "web".to_string(),
                                    timestamp,
                                };
                                let _ = tx.send(sync_msg);
                            }
                            WsMessage::Ping => {
                                // Respond with pong (handled by axum automatically for ws Ping frames)
                            }
                            _ => {}
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}
