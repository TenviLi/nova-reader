# Nova Reader - Architecture & Conventions

## Project Codename
Nova Reader ("Nova-Homelab-Reader")

## Core Philosophy
- **Local-first**: Data sovereignty on personal hardware
- **Performance-obsessed**: Rust backend + Svelte 5 fine-grained reactivity
- **AI-native**: Every feature leverages LLM intelligence
- **Zero-compromise UX**: Beautiful, fast, immersive reading experience

## Technology Decisions

### Backend: Rust + Axum
- Axum 0.8 with Tower middleware ecosystem
- Tokio async runtime for high-concurrency I/O
- SQLx for compile-time verified PostgreSQL queries
- PostgreSQL-backed durable task queue (`tasks` table with atomic claims)

### Frontend: Svelte 5
- Runes-only: $state, $derived, $effect (NO legacy syntax)
- $state.raw() for large immutable data (search results, graph data)
- Web Workers for heavy computation (parsing, network)
- Tailwind CSS v4 with design tokens
- i18n: paraglide-js (Inlang) — messages in apps/web/messages/{locale}.json
- UI: shadcn-svelte (bits-ui based, "nova" style)

### Database Layer
- PostgreSQL 16+: Core data, FTS, glossary, reading progress
- Qdrant: 2560-dim vector search (Cosine), collection `nova_chunks`
- Meilisearch: Full-text keyword + hybrid search (REST embedder → Gitee AI)
- Neo4j: Entity relationship graphs (GraphRAG)
- Redis: Caching and pub/sub; a future global queue migration may move durable task dispatch here

### AI Pipeline
- DeepSeek 4 Pro: Complex reasoning, translation, cleaning
- Qwen3-Embedding-4B: Remote via Gitee AI (`https://ai.gitee.com/v1/embeddings`, 2560 dim)
- Qwen3-Reranker-4B: Local vllm-mlx (`http://127.0.0.1:8000/v1/rerank`, Cohere-compatible)
- Semantic chunking: 512 tokens, 64 overlap, paragraph-aware boundaries (nova-embed/chunker.rs)

### Search Architecture
```
Query → Meilisearch (keyword + hybrid via REST embedder, semanticRatio=0.5)
      → Qdrant (vector similarity, 2560d, score_threshold=0.3)
      → RRF fusion (k=60)
      → Reranker (Qwen3-Reranker-4B, top_n)
      → Final results
```

## Architectural Rules

1. **No unwrap()** in production Rust code - all errors bubble via Result<T, E>
2. **No `export let`** in Svelte - only `$props()` rune
3. **Idempotent workers** - tasks must be safely retryable
4. **Event debouncing** - 500ms window for filesystem events
5. **Type boundaries** - Shared types defined in nova-core, used everywhere

## Directory Conventions
- `crates/nova-*/src/lib.rs` - Public API surface
- `crates/nova-*/src/error.rs` - Crate-specific error types
- `migrations/YYYYMMDDHHMMSS_description.sql` - Ordered migrations (do not rename applied migrations)
- `apps/web/src/lib/` - Shared frontend utilities
- `apps/web/src/routes/` - SvelteKit file-based routing

## Build & Run

### Quick Start (after reboot)
```bash
# 1. Start infrastructure (Postgres, Redis, Meilisearch, Qdrant, Neo4j)
docker compose up -d

# 2. Setup search infrastructure (Meilisearch + Qdrant indexes)
bash scripts/setup-search.sh

# 3. Build & run backend (port 3000)
cargo build --package nova-api && ./target/debug/nova-api

# 4. Frontend dev server with HMR (port 5173, proxies /api → :3000)
cd apps/web && pnpm dev

# 5. (Optional) Start reranker when model is downloaded
# vllm serve mlx-community/Qwen3-Reranker-4B-mxfp8 --port 8000 --hf_overrides '{"architectures":["Qwen3ForSequenceClassification"],"classifier_from_token":["no","yes"],"is_original_qwen3_reranker":true}'
```

### Access
- Frontend: http://localhost:5173
- API: http://localhost:3000/api
- Admin: username=admin, password=Admin123!

### Notes
- Backend has NO hot-reload — rebuild after Rust changes: `cargo build -p nova-api`
- Frontend has Vite HMR — edits auto-reflect in browser
- Docker services persist data in named volumes (survive restarts)
- Migrations run automatically on backend startup (idempotent)

### Production Build
```bash
# Backend
cargo build --package nova-api --release

# Frontend
cd apps/web && pnpm build
```

### Run all tests
```bash
cargo test --workspace
cd apps/web && pnpm test
```

## Library Choices (Don't Reinvent)

### Rust
| Task | Library | NOT |
|------|---------|-----|
| Glob matching | `glob-match` | Hand-rolled backtracking |
| AI/LLM calls | `async-openai` | Raw reqwest + JSON |
| File hashing | `sha2` + `hex` | Manual byte manipulation |
| Directory traversal | `walkdir` | Recursive `read_dir` |
| Document parsing | `kreuzberg` (91+ formats) | Per-format parsers |
| File watching | `notify` + `notify-debouncer-mini` | Polling |
| SSE streaming | `async-stream` + `axum::sse` | Manual stream impl |

### Frontend
| Task | Library | NOT |
|------|---------|-----|
| UI primitives | `bits-ui` (shadcn-svelte base) | Custom from scratch |
| Fuzzy search | `fuse.js` | Manual Levenshtein |
| Date handling | `dayjs` | Raw Date manipulation |
| Form validation | `zod` + `formsnap` + `sveltekit-superforms` | Manual validation |
| Data fetching | `@tanstack/svelte-query` v6 | Raw fetch + state |
| Query devtools | `@tanstack/svelte-query-devtools` | — |
| Toast notifications | `svelte-sonner` | Custom notification system |
| Dark mode | `mode-watcher` | Manual prefers-color-scheme |

## Key Endpoints

### AI Pipeline
- `POST /api/ai/batch-process` — Full-novel AI pipeline (summarize+entities+tags+style)
- `POST /api/ai/ingest-embeddings` — Generate & store text chunk vectors in Qdrant
- `POST /api/ai/chat` — RAG-augmented chat (Qdrant vectors + Neo4j graph context)
- `POST /api/ai/chat/stream` — SSE streaming chat

### Search
- `POST /api/search` — Hybrid search (Meilisearch + Qdrant + RRF fusion)
- Graph search uses Neo4j Cypher via `nova-graph::AgenticGraphRag`

## Immersive Translation (沉浸式翻译)
The reader supports 4 translation modes (ImmersiveTranslation.svelte):
- **原文**: Original text only
- **双语**: Bilingual (paragraph-by-paragraph interleaved)
- **译文**: Translation only
- **悬浮**: Hover to see translation tooltip

Uses glossary-aware batch translation with term consistency enforcement.

## TanStack Query Svelte v6 — 关键用法

项目使用 `@tanstack/svelte-query` **v6**（不是 v5），有以下关键区别：

### 1. Options 必须传 thunk（箭头函数）
```svelte
// ✅ 正确 — v6 要求传函数
const query = createQuery(() => ({
  queryKey: ['todos'],
  queryFn: () => fetchTodos(),
}));

// ❌ 错误 — v5 旧写法，v6 不支持
const query = createQuery({
  queryKey: ['todos'],
  queryFn: () => fetchTodos(),
});
```

### 2. 不使用 $ 前缀访问属性（非 store）
v6 返回的是**响应式对象**，不是 Svelte store，直接访问属性：
```svelte
<!-- ✅ 正确 — v6 直接访问 -->
{#if query.isLoading}...{/if}
{#each query.data as item}...{/each}

<!-- ❌ 错误 — v5 store 写法 -->
{#if $query.isLoading}...{/if}
{#each $query.data as item}...{/each}
```

### 3. Mutation 同理
```svelte
const mutation = createMutation(() => ({
  mutationFn: (data) => api.update(data),
}));

<!-- ✅ 正确 -->
<button onclick={() => mutation.mutate(data)} disabled={mutation.isPending}>

<!-- ❌ 错误 -->
<button onclick={() => $mutation.mutate(data)} disabled={$mutation.isPending}>
```

### 4. 响应式不需要 derived/writable
v6 自动追踪依赖，不需要像 v5 那样用 `derived` 包裹：
```svelte
let searchQuery = $state('');

// ✅ 直接引用 $state 变量，自动响应式
const results = createQuery(() => ({
  queryKey: ['search', searchQuery],
  queryFn: () => api.search(searchQuery),
}));
```

### 5. Devtools 已配置
`SvelteQueryDevtools` 已在 `+layout.svelte` 中全局启用，开发时右下角可见浮动按钮。
