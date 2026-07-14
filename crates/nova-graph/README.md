# nova-graph

Agentic GraphRAG 知识图谱引擎，基于 Neo4j。

## 职责

- **实体存储**: MERGE 创建/更新实体节点 (人物、地点、物品、组织等)
- **关系映射**: 实体间关系建模 (主从、敌对、师徒、血缘...)
- **社区检测**: Leiden 算法发现实体社群 (用于 GraphRAG 总结)
- **多跳推理**: 最短路径查找, 关系链路追踪
- **图谱 RAG**: 结合图结构为 AI 提供上下文

## 模块

- `neo4j.rs` — Neo4j HTTP 客户端 (Cypher 查询)
- `entity.rs` — 实体 CRUD 操作
- `community.rs` — 社区检测与总结

## GraphRAG 流程

```
用户提问 → 实体识别 → 图谱检索相关实体/关系 → 
          社区总结注入 → LLM 生成回答
```

## Agentic GraphRAG

支持 agent 式多步推理：
1. **分解问题** → 识别涉及的实体和关系类型
2. **图谱遍历** → 根据问题类型选择 1-hop/2-hop/shortest path
3. **上下文聚合** → 合并节点属性 + 关系描述 + 社区摘要
4. **生成回答** → 将结构化图信息注入 LLM prompt

## 使用

```rust
use nova_graph::Neo4jClient;

let client = Neo4jClient::new("bolt://localhost:7687", "neo4j", "password");

// 获取书籍的完整知识图谱
let graph = client.get_book_graph("book-uuid").await?;

// 多跳推理：查找两个角色的关系路径
let paths = client.find_paths("张三", "李四", 3).await?;
```
