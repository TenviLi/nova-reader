//! Agentic GraphRAG — Multi-step reasoning over knowledge graphs.
//!
//! Implements the Microsoft GraphRAG paper's approach:
//! 1. Entity extraction → Neo4j nodes
//! 2. Relationship mapping → Neo4j edges
//! 3. Community detection (Leiden) → hierarchical clusters
//! 4. Community summarization → LLM-generated descriptions
//! 5. Query-time: decompose question → graph retrieval → context aggregation → answer
//!
//! "Agentic" means the system can plan multi-step retrieval strategies
//! based on question complexity.

use serde::{Deserialize, Serialize};

/// The agentic GraphRAG query planner.
/// Decides retrieval strategy based on question type.
pub struct AgenticGraphRag {
    /// Strategy for how to retrieve context
    strategy: RetrievalStrategy,
}

/// Different retrieval strategies based on query complexity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetrievalStrategy {
    /// Direct entity lookup + 1-hop neighbors
    Local { entity_names: Vec<String> },
    /// Community-level summarization for broad questions
    Global { community_level: i32 },
    /// Multi-hop path finding between entities
    MultiHop { source: String, target: String, max_hops: i32 },
    /// Hybrid: local entities + community context
    Hybrid { entity_names: Vec<String>, include_community: bool },
}

/// Result from an agentic GraphRAG query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRagResult {
    pub strategy_used: String,
    pub context: String,
    pub entities_involved: Vec<String>,
    pub community_summaries: Vec<String>,
    pub paths: Vec<Vec<String>>,
    pub confidence: f64,
}

/// Plan what retrieval strategy to use based on the user's question.
/// This is the "agentic" part — the system reasons about HOW to retrieve.
pub const PLANNING_PROMPT: &str = r#"你是一个检索策略规划器。根据用户问题，决定从知识图谱中检索信息的最佳策略。

可用策略：
1. "local" — 查找特定实体及其直接关系（适合：某个角色的属性/关系）
2. "global" — 获取社区级别的总结（适合：主题概述/全局问题）
3. "multi_hop" — 查找两个实体之间的路径（适合：两者关系/因果链）
4. "hybrid" — 结合实体和社区信息（适合：复杂分析问题）

用户问题: {question}

已知实体列表(部分): {known_entities}

输出JSON:
```json
{
  "strategy": "local|global|multi_hop|hybrid",
  "entity_names": ["实体1", "实体2"],
  "reasoning": "选择此策略的原因"
}
```
"#;

/// Context aggregation prompt — merges graph data into coherent context for the final LLM.
pub const AGGREGATION_PROMPT: &str = r#"基于以下从知识图谱中检索到的信息，为回答用户问题提供结构化上下文。

## 实体信息
{entity_context}

## 社区摘要
{community_context}

## 关系路径
{path_context}

请将以上信息整理为简洁的参考上下文（不超过500字），重点包含与问题最相关的信息。
"#;

impl AgenticGraphRag {
    pub fn new(strategy: RetrievalStrategy) -> Self {
        Self { strategy }
    }

    /// Get the Cypher queries needed for this strategy.
    pub fn get_cypher_queries(&self, book_id: &str) -> Vec<(String, serde_json::Value)> {
        match &self.strategy {
            RetrievalStrategy::Local { entity_names } => {
                vec![(
                    "MATCH (n {book_id: $book_id})-[r]-(m) WHERE n.name IN $names RETURN n, r, m LIMIT 50".to_string(),
                    serde_json::json!({ "book_id": book_id, "names": entity_names }),
                )]
            }
            RetrievalStrategy::Global { community_level } => {
                vec![(
                    "MATCH (c:Community {book_id: $book_id, level: $level}) RETURN c.summary, c.key_findings LIMIT 10".to_string(),
                    serde_json::json!({ "book_id": book_id, "level": community_level }),
                )]
            }
            RetrievalStrategy::MultiHop { source, target, max_hops } => {
                vec![(
                    format!(
                        "MATCH path = shortestPath((a {{book_id: $book_id, name: $source}})-[*..{}]->(b {{name: $target}})) RETURN path",
                        max_hops
                    ),
                    serde_json::json!({ "book_id": book_id, "source": source, "target": target }),
                )]
            }
            RetrievalStrategy::Hybrid { entity_names, include_community } => {
                let mut queries = vec![(
                    "MATCH (n {book_id: $book_id})-[r]-(m) WHERE n.name IN $names RETURN n, r, m LIMIT 30".to_string(),
                    serde_json::json!({ "book_id": book_id, "names": entity_names }),
                )];
                if *include_community {
                    queries.push((
                        "MATCH (c:Community {book_id: $book_id}) WHERE ANY(e IN c.entities WHERE e IN $names) RETURN c.summary LIMIT 5".to_string(),
                        serde_json::json!({ "book_id": book_id, "names": entity_names }),
                    ));
                }
                queries
            }
        }
    }
}
