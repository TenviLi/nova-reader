# nova-search

混合搜索引擎 — 关键词 + 语义向量 + 图谱推理 + RRF 融合。

## 架构

```
┌─────────────────────────────────────────────┐
│          HybridSearchEngine                  │
├──────────┬──────────┬──────────┬────────────┤
│ Keyword  │ Semantic │ Sparse   │ Graph      │
│ Meili    │ Qdrant   │ BM25     │ Neo4j      │
└──────────┴──────────┴──────────┴────────────┘
                     │
              ┌──────┴──────┐
              │  RRF Fusion │
              └─────────────┘
```

## 模块

- `meilisearch.rs` — Meilisearch 全文检索客户端
- `qdrant.rs` — Qdrant 稠密/稀疏向量搜索
- `rrf.rs` — Reciprocal Rank Fusion 结果融合算法
- `hybrid.rs` — 统一搜索编排器
- `embedding.rs` — 嵌入服务客户端 (Qwen3-Embedding via MLX)

## 搜索模式

| 模式 | 说明 | 适用场景 |
|------|------|---------|
| `Keyword` | Meilisearch 全文检索 | 精确关键词, 书名/作者 |
| `Semantic` | Qdrant 向量相似度 | 语义理解, "类似主题的书" |
| `Hybrid` | Keyword + Semantic + RRF | 默认模式, 兼顾精确和语义 |
| `Graph` | Neo4j Cypher 图遍历 | 实体关系, "某角色相关的情节" |

## 使用

```rust
use nova_search::HybridSearchEngine;
use nova_core::domain::search::{SearchQuery, SearchMode};

let engine = HybridSearchEngine::new(meilisearch, qdrant);
let results = engine.search(&SearchQuery {
    query: "修仙突破金丹".to_string(),
    mode: SearchMode::Hybrid,
    limit: 20,
    ..Default::default()
}).await?;
```
