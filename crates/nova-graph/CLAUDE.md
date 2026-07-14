# nova-graph

Agentic GraphRAG engine built on Neo4j. Entity extraction, relationship mapping, community detection (Leiden algorithm), and multi-hop reasoning over knowledge graphs.

## Architecture

```
src/
├── lib.rs         — Re-exports
├── neo4j.rs       — Neo4jClient (HTTP transactional API)
├── agentic.rs     — AgenticGraphRag (multi-hop reasoning)
├── entity.rs      — Entity type definitions
└── community.rs   — Leiden community detection
```

## Key Types

```rust
pub struct Neo4jClient {
    pub async fn execute(&self, cypher: &str, params: Option<Value>) -> Result<Value>;
    pub async fn upsert_entity(&self, entity_type: &str, name: &str, book_id: &str, props: &Value) -> Result<()>;
    pub async fn create_relationship(&self, source: &str, rel_type: &str, target: &str) -> Result<()>;
}

pub struct AgenticGraphRag {
    pub async fn reason(&self, query: &str, limit: usize) -> Result<Vec<ReasoningPath>>;
}
```

## Graph Schema

```cypher
(:Person {name, aliases, description, book_id})
(:Place {name, description, book_id})
(:Organization {name, description, book_id})
(:Event {name, timestamp, book_id})

-[:APPEARS_IN {chapter_id, frequency}]->
-[:RELATED_TO {type, strength}]->
-[:LOCATED_IN]->
-[:MEMBER_OF]->
```

## Key Patterns

- **REST over Bolt**: Uses Neo4j HTTP transactional endpoint (simpler, no driver needed)
- **Parameter binding**: `$param` placeholders prevent Cypher injection
- **Upsert semantics**: MERGE for entities, CREATE for relationships
- **Community detection**: Leiden algorithm clusters related entities
- **Multi-hop reasoning**: BFS/DFS over graph to find evidence paths
- **Stateless**: Each query is independent, no session state

## Dependencies

- **Internal**: nova-core
- **External**: reqwest, base64, serde, serde_json

## Build & Test

```bash
cargo build -p nova-graph
cargo test -p nova-graph
```

## Environment

Requires: Neo4j 5 Community (port 7474/7687)
