use std::sync::Arc;

use anyhow::Context;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// mod ai_service;
mod access;
mod ai_usage;
mod auth;
mod config;
mod dedup;
mod error;
mod extractors;
mod feature_flags;
mod middleware;
mod migrations;
mod prompts;
mod repo;
mod routes;
mod state;
mod task_queue;
mod task_worker;
#[cfg(test)]
mod tests;

use config::AppConfig;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment
    dotenvy::dotenv().ok();

    // Load configuration early to decide log format
    let config = AppConfig::from_env().context("Failed to load configuration")?;

    // Initialize structured logging
    // Production: JSON format for log aggregation
    // Development: Pretty-printed human-readable format
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "nova_api=debug,tower_http=debug".into());

    if config.env == config::Environment::Production {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE),
            )
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().with_target(true))
            .init();
    }

    if config.tls_configured() {
        info!(
            "TLS certificate paths are configured; native TLS listener is not enabled in this binary, terminate TLS at a reverse proxy"
        );
    }

    let bind_addr = format!("{}:{}", config.host, config.port);

    // Initialize application state
    let state = AppState::new(config)
        .await
        .context("Failed to initialize app state")?;
    let state = Arc::new(state);

    // Verify external AI service connectivity (non-blocking)
    verify_ai_services(&state).await;

    // Spawn background task worker
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let _worker_handle = task_worker::spawn_worker(state.clone(), shutdown_rx);

    // Build router
    routes::init_health();
    let app = routes::build_router(state);

    // Start server with graceful shutdown
    let listener = TcpListener::bind(&bind_addr)
        .await
        .context("Failed to bind to address")?;

    info!("🚀 Nova Reader API listening on {}", bind_addr);
    info!("   Dashboard: http://{}/", bind_addr);
    info!("   API Docs:  http://{}/api/docs", bind_addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Server error")?;

    // Signal worker to shut down
    let _ = shutdown_tx.send(true);

    info!("Server shutdown gracefully");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!("Received Ctrl+C, shutting down..."),
        _ = terminate => info!("Received SIGTERM, shutting down..."),
    }
}

/// Check reranker and embedding endpoints are reachable at startup.
/// Logs warnings but does not block startup (graceful degradation).
async fn verify_ai_services(state: &Arc<AppState>) {
    use tracing::warn;

    let client = &state.http_client;
    let timeout = std::time::Duration::from_secs(5);

    // Check embedding endpoint
    if !state.config.embedding_endpoint.is_empty() {
        let url = format!(
            "{}/v1/models",
            state.config.embedding_endpoint.trim_end_matches('/')
        );
        match client.get(&url).timeout(timeout).send().await {
            Ok(resp) if resp.status().is_success() => {
                info!(
                    "✓ Embedding service reachable at {}",
                    state.config.embedding_endpoint
                );
            }
            Ok(resp) => {
                warn!("⚠ Embedding service returned {} at {}", resp.status(), url);
            }
            Err(e) => {
                warn!("⚠ Embedding service unreachable at {}: {}", url, e);
            }
        }
    }

    // Check reranker endpoint
    if !state.config.reranker_endpoint.is_empty() {
        let url = format!(
            "{}/v1/models",
            state.config.reranker_endpoint.trim_end_matches('/')
        );
        match client.get(&url).timeout(timeout).send().await {
            Ok(resp) if resp.status().is_success() => {
                info!(
                    "✓ Reranker service reachable at {}",
                    state.config.reranker_endpoint
                );
            }
            Ok(resp) => {
                warn!("⚠ Reranker service returned {} at {}", resp.status(), url);
            }
            Err(e) => {
                warn!("⚠ Reranker service unreachable at {}: {}", url, e);
            }
        }
    }
}
