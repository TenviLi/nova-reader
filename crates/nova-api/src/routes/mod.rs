use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    http::{HeaderValue, Method},
    middleware as axum_middleware, Router,
};
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};

use crate::middleware::{
    ai_rate_limit, graceful_degradation, per_user_ai_rate_limit, require_auth, validate_request,
    wrap_response,
};
use crate::state::AppState;

// Core routes (always enabled)
mod auth;
mod health;

// Sprint 1: Book library browsing
mod books;
mod chapters;
mod duplicates;
mod libraries;

// Sprint 2: Reading experience
mod annotations;
mod collections;
mod persons;
mod progress;
mod shelves;
mod stats;

// Sprint 3: Search + Settings + Tasks + Entities
mod entities;
mod search;
mod settings;
mod tasks;

// Sprint 4: Admin + Recommendations
mod admin;
mod notifications;
mod recommendations;

// Sprint 5: AI + Glossary (real implementations)
mod ai;
mod ai_usage;
mod analysis;
mod glossary_translate;
mod translate;

// Sprint 8: Semantic Intelligence
mod semantic_tags;

// Sprint 9: Trope Ontology + Persona Drift + Rule Splicing
mod trope_ontology;

// Sprint 6: Additional routes (blocked on DB schema — need migrations for custom types/columns)
// mod calibre;
// mod import_sources;
// mod metrics;
// mod opds;
// mod plugins;
// mod social;
// mod webhooks;
// mod ws;

/// Initialize health-related startup state.
pub fn init_health() {
    health::init_start_time();
}

/// Build the complete application router with all routes and middleware.
pub fn build_router(state: Arc<AppState>) -> Router {
    // Public API routes (no auth required)
    let public_routes = Router::new()
        .merge(health::routes())
        .merge(auth::routes())
        .merge(annotations::public_routes());

    let ai_routes = ai::router()
        .route_layer(axum_middleware::from_fn_with_state(
            state.clone(),
            per_user_ai_rate_limit,
        ))
        .route_layer(axum_middleware::from_fn_with_state(
            state.clone(),
            ai_rate_limit,
        ));

    let translate_routes = translate::router()
        .route_layer(axum_middleware::from_fn_with_state(
            state.clone(),
            per_user_ai_rate_limit,
        ))
        .route_layer(axum_middleware::from_fn_with_state(
            state.clone(),
            ai_rate_limit,
        ));

    // Protected routes - require authentication
    let protected_routes = Router::new()
        .merge(books::routes())
        .merge(chapters::routes())
        .merge(duplicates::routes())
        .merge(libraries::routes())
        .merge(persons::routes())
        .merge(annotations::routes())
        .merge(progress::routes())
        .merge(stats::routes())
        .merge(collections::routes())
        .merge(shelves::routes())
        .merge(search::routes())
        .merge(settings::routes())
        .merge(tasks::routes())
        .merge(entities::routes())
        .merge(admin::routes())
        .merge(notifications::routes())
        .merge(recommendations::routes())
        .nest("/ai", ai_routes)
        .nest("/ai/usage", ai_usage::router())
        .merge(translate_routes)
        .merge(glossary_translate::routes())
        .merge(analysis::routes())
        .merge(semantic_tags::routes())
        .merge(trope_ontology::routes())
        .route_layer(axum_middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ));

    let api_routes = Router::new().merge(public_routes).merge(protected_routes);

    Router::new()
        .nest("/api", api_routes)
        .layer(axum_middleware::from_fn(wrap_response))
        .layer(axum_middleware::from_fn(validate_request))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            graceful_degradation,
        ))
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        .layer(CompressionLayer::new())
        .layer({
            let origins: Vec<HeaderValue> = state
                .config
                .cors_origins
                .iter()
                .filter_map(|o| o.parse().ok())
                .collect();
            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PUT,
                    Method::PATCH,
                    Method::DELETE,
                    Method::OPTIONS,
                ])
                .allow_headers([
                    axum::http::header::CONTENT_TYPE,
                    axum::http::header::AUTHORIZATION,
                    axum::http::header::ACCEPT,
                    axum::http::header::COOKIE,
                ])
                .allow_credentials(true)
        })
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
