mod diff;
mod resolution;
mod scan;

use serde_json::Value;
use sha2::Digest;
use uuid::Uuid;

use nova_core::domain::dedup::DedupTaskPayload;

pub(crate) use nova_core::domain::dedup::ResolveAction;
pub(crate) use resolution::resolve_pair;
pub(crate) use scan::{enqueue_incremental_scan, enqueue_scan};

pub(crate) use diff::compare_chapter_texts;

use crate::state::AppState;

pub(crate) const ALGORITHM_VERSION: i32 = nova_ingest::dedup::DEDUP_ALGORITHM_VERSION as i32;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DedupTaskError {
    Failed(String),
    Continue(String),
}

/// Payload schema for chunk vectors consumed by semantic duplicate discovery.
/// Bump this whenever the freshness fields or their meaning changes.
pub(crate) const EMBEDDING_PAYLOAD_VERSION: u64 = 1;

/// Qdrant accepts only unsigned integers or UUIDs as point IDs. Derive one
/// stable u64 namespace for every writer so API and durable-worker rebuilds
/// replace the same logical chunk instead of creating parallel point sets.
pub(crate) fn embedding_point_id(book_id: Uuid, chapter_index: i32, chunk_index: usize) -> u64 {
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"nova_chunks:embedding-point:v1");
    hasher.update(book_id.as_bytes());
    hasher.update(chapter_index.to_be_bytes());
    hasher.update(u64::try_from(chunk_index).unwrap_or(u64::MAX).to_be_bytes());
    let digest = hasher.finalize();
    let mut point_id = [0_u8; 8];
    point_id.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(point_id)
}

/// The immutable contract attached to every Qdrant chunk for one book build.
///
/// The source hash covers every current chapter, including chapters suppressed
/// from downstream indexes by a confirmed duplicate resolution. That keeps the
/// vector snapshot comparable with `book_fingerprints`, which uses the same
/// ordered, byte-exact source hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EmbeddingFreshnessContract {
    source_content_hash: String,
    embedding_model: String,
    embedding_dimensions: usize,
}

impl EmbeddingFreshnessContract {
    pub(crate) fn from_source_content_hash(
        source_content_hash: String,
        embedding_model: impl Into<String>,
        embedding_dimensions: usize,
    ) -> Self {
        Self {
            source_content_hash,
            embedding_model: embedding_model.into(),
            embedding_dimensions,
        }
    }

    pub(crate) fn chunk_payload(
        &self,
        book_id: Uuid,
        book_title: Option<&str>,
        chapter_index: i32,
        chapter_title: &str,
        chunk_index: usize,
        text: &str,
    ) -> Value {
        let mut payload = serde_json::json!({
            "book_id": book_id.to_string(),
            "chapter_index": chapter_index,
            "chapter_number": chapter_index,
            "chapter_title": chapter_title,
            "chunk_index": chunk_index,
            "text": text,
            "book_source_content_hash": self.source_content_hash,
            "embedding_model": self.embedding_model,
            "embedding_dimensions": self.embedding_dimensions,
            "embedding_payload_version": EMBEDDING_PAYLOAD_VERSION,
        });
        if let Some(book_title) = book_title {
            payload["book_title"] = Value::String(book_title.to_string());
        }
        payload
    }

    pub(crate) fn matches_qdrant_point(&self, point: &Value, book_id: Uuid) -> bool {
        let Some(payload) = point.get("payload") else {
            return false;
        };
        let payload_book_id = payload
            .get("book_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok());
        let expected_dimensions = u64::try_from(self.embedding_dimensions).ok();

        payload_book_id == Some(book_id)
            && payload
                .get("book_source_content_hash")
                .and_then(Value::as_str)
                == Some(self.source_content_hash.as_str())
            && payload.get("embedding_model").and_then(Value::as_str)
                == Some(self.embedding_model.as_str())
            && payload.get("embedding_dimensions").and_then(Value::as_u64) == expected_dimensions
            && payload
                .get("embedding_payload_version")
                .and_then(Value::as_u64)
                == Some(EMBEDDING_PAYLOAD_VERSION)
    }

    pub(crate) fn qdrant_common_filter_conditions(&self) -> Vec<Value> {
        vec![
            serde_json::json!({
                "key": "embedding_model",
                "match": { "value": self.embedding_model },
            }),
            serde_json::json!({
                "key": "embedding_dimensions",
                "match": { "value": self.embedding_dimensions },
            }),
            serde_json::json!({
                "key": "embedding_payload_version",
                "match": { "value": EMBEDDING_PAYLOAD_VERSION },
            }),
        ]
    }

    pub(crate) fn qdrant_book_filter_conditions(&self, book_id: Uuid) -> Vec<Value> {
        let mut conditions = vec![serde_json::json!({
            "key": "book_id",
            "match": { "value": book_id.to_string() },
        })];
        conditions.push(serde_json::json!({
            "key": "book_source_content_hash",
            "match": { "value": self.source_content_hash },
        }));
        conditions.extend(self.qdrant_common_filter_conditions());
        conditions
    }
}

pub(crate) async fn load_embedding_freshness_contract(
    chapters: &crate::repo::pg_chapter::PgChapterRepository,
    book_id: Uuid,
    embedding_model: &str,
    embedding_dimensions: usize,
) -> Result<EmbeddingFreshnessContract, String> {
    let source_content_hash = chapters
        .book_source_content_hash(book_id)
        .await
        .map_err(|error| format!("failed to load book source for embedding freshness: {error}"))?;
    Ok(EmbeddingFreshnessContract::from_source_content_hash(
        source_content_hash,
        embedding_model,
        embedding_dimensions,
    ))
}

/// Delete-and-wait is required before a rebuild. Without `wait=true`, Qdrant
/// may apply the asynchronous delete after the new upserts and erase fresh
/// points that reused deterministic IDs.
pub(crate) async fn delete_book_embedding_points(
    client: &reqwest::Client,
    qdrant_url: &str,
    book_id: Uuid,
) -> Result<(), String> {
    let response = client
        .post(format!(
            "{}/collections/nova_chunks/points/delete?wait=true",
            qdrant_url.trim_end_matches('/')
        ))
        .json(&serde_json::json!({
            "filter": {
                "must": [{
                    "key": "book_id",
                    "match": { "value": book_id.to_string() }
                }]
            }
        }))
        .send()
        .await
        .map_err(|error| format!("Qdrant book-vector delete failed: {error}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Qdrant book-vector delete returned {status}: {body}"
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) static DEDUP_DATABASE_TEST_LOCK: tokio::sync::Mutex<()> =
    tokio::sync::Mutex::const_new(());

pub(crate) async fn execute_task(
    state: &AppState,
    task_id: Uuid,
    payload: &Value,
) -> Result<Option<Value>, DedupTaskError> {
    let payload: DedupTaskPayload = serde_json::from_value(payload.clone()).map_err(|error| {
        DedupTaskError::Failed(format!("invalid deduplicate task payload: {error}"))
    })?;
    match payload {
        DedupTaskPayload::Scan(scan_task) => scan::execute_scan(state, task_id, &scan_task)
            .await
            .map_err(DedupTaskError::Failed),
        DedupTaskPayload::IndexCleanup(cleanup_task) => {
            resolution::execute_index_cleanup(state, task_id, &cleanup_task).await
        }
    }
}

#[cfg(test)]
mod embedding_freshness_tests {
    use super::*;

    #[test]
    fn shared_chunk_payload_carries_and_validates_the_full_freshness_contract() {
        let book_id = Uuid::from_u128(7);
        let contract = EmbeddingFreshnessContract::from_source_content_hash(
            "current-source-hash".to_string(),
            "embedding-model-v2",
            2_560,
        );
        let payload = contract.chunk_payload(book_id, Some("Book"), 3, "Chapter", 4, "Text");
        let point = serde_json::json!({ "payload": payload });

        assert!(contract.matches_qdrant_point(&point, book_id));
        assert_eq!(
            point["payload"]["embedding_payload_version"],
            EMBEDDING_PAYLOAD_VERSION
        );
        assert_eq!(point["payload"]["embedding_dimensions"], 2_560);
    }

    #[test]
    fn shared_embedding_point_ids_are_valid_stable_u64_values() {
        let book_id = Uuid::from_u128(9);
        let point_id = embedding_point_id(book_id, 3, 4);

        assert_eq!(point_id, embedding_point_id(book_id, 3, 4));
        assert_ne!(point_id, embedding_point_id(book_id, 3, 5));
        assert_ne!(point_id, embedding_point_id(book_id, 4, 4));
        assert_eq!(serde_json::json!(point_id).as_u64(), Some(point_id));
    }
}
