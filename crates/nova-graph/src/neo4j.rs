use reqwest::Client;
use serde::Serialize;

/// Client for Neo4j HTTP API (transactional endpoint).
pub struct Neo4jClient {
    client: Client,
    base_url: String,
    auth: String,
}

#[derive(Debug, Serialize)]
struct CypherRequest {
    statements: Vec<CypherStatement>,
}

#[derive(Debug, Serialize)]
struct CypherStatement {
    statement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<serde_json::Value>,
}

impl Neo4jClient {
    pub fn new(uri: &str, user: &str, password: &str) -> Self {
        use base64::Engine;
        let auth = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", user, password));

        // Convert bolt:// to http:// for REST API
        let base_url = uri
            .replace("bolt://", "http://")
            .replace(":7687", ":7474");

        Self {
            client: Client::new(),
            base_url,
            auth,
        }
    }

    /// Execute a Cypher query.
    pub async fn execute(
        &self,
        cypher: &str,
        params: Option<serde_json::Value>,
    ) -> nova_core::Result<serde_json::Value> {
        let url = format!("{}/db/neo4j/tx/commit", self.base_url);

        let request = CypherRequest {
            statements: vec![CypherStatement {
                statement: cypher.to_string(),
                parameters: params,
            }],
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Basic {}", self.auth))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| nova_core::Error::GraphDb(e.to_string()))?;

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| nova_core::Error::GraphDb(e.to_string()))?;

        Ok(body)
    }

    /// Create or merge an entity node.
    pub async fn upsert_entity(
        &self,
        entity_type: &str,
        name: &str,
        book_id: &str,
        properties: &serde_json::Value,
    ) -> nova_core::Result<()> {
        let cypher = format!(
            "MERGE (n:{} {{name: $name, book_id: $book_id}}) \
             SET n += $props \
             RETURN n",
            entity_type
        );

        let params = serde_json::json!({
            "name": name,
            "book_id": book_id,
            "props": properties,
        });

        self.execute(&cypher, Some(params)).await?;
        Ok(())
    }

    /// Create a relationship between two entities.
    pub async fn create_relationship(
        &self,
        source_name: &str,
        target_name: &str,
        relation_type: &str,
        properties: &serde_json::Value,
    ) -> nova_core::Result<()> {
        let cypher = format!(
            "MATCH (a {{name: $source}}), (b {{name: $target}}) \
             MERGE (a)-[r:{} {{}}]->(b) \
             SET r += $props \
             RETURN r",
            relation_type
        );

        let params = serde_json::json!({
            "source": source_name,
            "target": target_name,
            "props": properties,
        });

        self.execute(&cypher, Some(params)).await?;
        Ok(())
    }

    /// Get all entities and relationships for a book.
    pub async fn get_book_graph(&self, book_id: &str) -> nova_core::Result<serde_json::Value> {
        let cypher = "MATCH (n {book_id: $book_id})-[r]->(m) \
                      RETURN n, r, m LIMIT 500";

        let params = serde_json::json!({ "book_id": book_id });
        self.execute(cypher, Some(params)).await
    }

    /// Find paths between two entities (multi-hop reasoning).
    pub async fn find_paths(
        &self,
        source: &str,
        target: &str,
        max_hops: i32,
    ) -> nova_core::Result<serde_json::Value> {
        let cypher = format!(
            "MATCH path = shortestPath((a {{name: $source}})-[*..{}]->(b {{name: $target}})) \
             RETURN path",
            max_hops
        );

        let params = serde_json::json!({
            "source": source,
            "target": target,
        });

        self.execute(&cypher, Some(params)).await
    }
}
