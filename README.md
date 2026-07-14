# Nova Reader

> 极致性能的个人数字文学智库 — A high-performance personal digital literary intelligence platform.

Nova Reader 是一个面向个人 Homelab 部署的小说管理与 AI 阅读平台。它将海量小说文本转化为结构化知识，支持全文搜索、语义检索、人物关系图谱、跨语言翻译，以及基于 AI 的创作辅助。

## ✨ 核心能力

- **智能摄入** — 监控目录自动导入，支持 TXT/EPUB/PDF/DOCX 等格式，AI 驱动的乱码清洗
- **全文搜索** — PostgreSQL FTS + Qdrant 向量混合检索，RRF 融合排序
- **GraphRAG** — Neo4j 驱动的人物关系、事件时间线、因果推理
- **语义去重** — MinHash LSH 句法阻断 + 深层语义去重双轨引擎
- **AI 翻译** — 术语表感知的上下文连贯翻译
- **创作辅助** — 相似情节发现、素材调取、风格分析
- **沉浸阅读** — CFI 原子级进度追踪、流式分章加载

## 🏗️ 技术架构

```
┌─────────────────────────────────────────────────────────┐
│                    Svelte 5 Frontend                      │
│         (Runes • Tailwind v4 • Web Workers)              │
└────────────────────────┬────────────────────────────────┘
                         │ REST / SSE / WebSocket
┌────────────────────────┴────────────────────────────────┐
│                   Rust Backend (Axum)                     │
│              Tokio • Tower • SQLx • Redis                │
├──────────┬──────────┬──────────┬──────────┬─────────────┤
│ Ingestion│  Search  │  Graph   │Translate │   Worker    │
│ Service  │  Engine  │  RAG     │ Engine   │   Queue     │
└────┬─────┴────┬─────┴────┬─────┴────┬─────┴──────┬──────┘
     │          │          │          │            │
┌────┴───┐ ┌───┴───┐ ┌───┴───┐ ┌───┴────┐ ┌────┴────┐
│ PostgreSQL│ │Qdrant │ │ Neo4j │ │DeepSeek│ │  Redis  │
│   16+  │ │Vector │ │ Graph │ │  API   │ │  Queue  │
└────────┘ └───────┘ └───────┘ └────────┘ └─────────┘
```

## 🚀 快速开始

### 前置要求
- Docker & Docker Compose
- Rust 1.82+ (`rustup`)
- Node.js 22+ & pnpm 9+ (推荐 fnm/nvm 管理)

### 开发启动

```bash
# 1. 启动基础设施 (PostgreSQL, Redis, Meilisearch, Qdrant, Neo4j)
docker compose up -d

# 2. 构建并启动后端 (port 3000，自动运行迁移)
cargo build -p nova-api && ./target/debug/nova-api

# 3. 启动前端 (port 5173, Vite HMR 热更新, /api 代理到 :3000)
cd apps/web && pnpm dev
```

访问 http://localhost:5173 即可使用。默认管理员：`admin` / `Admin123!`

### 开发须知

| 组件 | 热更新 | 重启方式 |
|------|--------|----------|
| 前端 (Svelte) | ✅ Vite HMR | 自动 |
| 后端 (Rust) | ❌ 需重新编译 | `cargo build -p nova-api && ./target/debug/nova-api` |
| Docker 服务 | — | `docker compose restart <service>` |

### 生产部署

```bash
# 一键启动 (含 API + Worker + Frontend + 所有依赖服务)
docker compose -f docker-compose.prod.yml up -d
```

详见 [部署指南](docs/deployment.md)。

## 📁 项目结构

```
nova-reader/
├── apps/web/              # Svelte 5 前端应用
├── crates/
│   ├── nova-core/         # 核心领域模型与共享类型
│   ├── nova-api/          # Axum HTTP/WebSocket 服务
│   ├── nova-worker/       # 异步任务队列 Worker
│   ├── nova-ingest/       # 文件摄入与解析引擎
│   ├── nova-search/       # 混合搜索与 RAG 引擎
│   ├── nova-graph/        # GraphRAG 与 Neo4j 集成
│   ├── nova-translate/    # 术语感知翻译引擎
│   └── nova-embed/        # 嵌入向量服务
├── docker-compose.yml     # 基础设施编排
├── migrations/            # PostgreSQL 迁移
├── docs/                  # 架构文档
└── scripts/               # 工具脚本
```

## 📋 系统要求

- macOS (Apple Silicon) / Linux
- Docker & Docker Compose
- Rust 1.82+ (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- Node.js 22+ & pnpm 9+ (`corepack enable && corepack prepare pnpm@latest --activate`)
- 16GB+ RAM (推荐 32GB+)

## License

MIT
