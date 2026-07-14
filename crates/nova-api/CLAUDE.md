# nova-api

HTTP API server built with Axum that orchestrates all other crates. Exposes 150+ RESTful endpoints for books, chapters, search, translations, embeddings, and integrates JWT authentication, rate limiting, and WebSocket support for multi-device sync.

## Architecture

```
src/
├── main.rs          — Server bootstrap, graceful shutdown
├── state.rs         — AppState (DB pools, repos, services)
├── routes/mod.rs    — Route tree assembly + middleware layers
├── routes/*.rs      — 30+ domain handler modules
├── middleware.rs    — Auth, rate limiting, validation, degradation
├── auth/            — JWT token generation/validation
├── repo/            — PostgreSQL repository implementations
└── ai_service.rs    — DeepSeek/OpenRouter LLM integration
```

## Key Patterns

- **State injection**: `Arc<AppState>` via Axum `State` extractor
- **Repository pattern**: All DB access through typed repos (`PgBookRepository`, etc.)
- **Layered middleware**: validate_request → rate_limit → auth → graceful_degradation
- **Error handling**: `ApiError(nova_core::Error)` → JSON `{ error: { code, message, retryable } }`
- **Rate limiting (Governor)**: Global 30 req/min + per-user 60 req/min for AI endpoints
- **Dual auth**: `Authorization: Bearer <token>` header or `nova_token` cookie

## Response Patterns

| Pattern | Usage |
|---------|-------|
| `Json<T>` | Single resources |
| `Json<Vec<T>>` | Lists |
| `Json<PaginatedResponse<T>>` | Paginated (data/total/page/per_page/total_pages) |
| `Sse<Stream<Event>>` | Streaming (AI chat, tasks) |
| `WebSocket` | Real-time progress sync |

## Dependencies

- **Internal**: nova-core, nova-ingest, nova-search, nova-worker, nova-embed, nova-graph, nova-translate
- **External**: axum, tower, sqlx, redis, jsonwebtoken, argon2, governor, async-openai

## Build & Test

```bash
cargo build -p nova-api
cargo test -p nova-api
cargo run -p nova-api  # Starts server on :3000
```

## Environment

Requires: PostgreSQL, Redis, Meilisearch, Qdrant, Neo4j (see docker-compose.yml)
