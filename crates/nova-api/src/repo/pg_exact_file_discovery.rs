use chrono::{DateTime, Utc};
use nova_core::{domain::dedup_discovery::ExactFileDiscoverySource, Error, Result};
use sha2::{Digest, Sha256};
use std::str::FromStr;
use uuid::Uuid;

use super::pg_duplicate::PgDuplicateRepository;

#[derive(Debug, Clone)]
pub(crate) struct RecordExactFileDiscovery<'a> {
    pub(crate) matched_book_id: Uuid,
    pub(crate) source: ExactFileDiscoverySource,
    pub(crate) source_path: &'a str,
    pub(crate) file_hash: &'a str,
    pub(crate) file_size_bytes: i64,
    pub(crate) discovered_by: Uuid,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExactFileDiscoveryFilter<'a> {
    pub(crate) visible_library_ids: Option<&'a [Uuid]>,
    pub(crate) library_id: Option<Uuid>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ExactFileDiscoveryRecord {
    #[serde(skip)]
    pub(crate) total_count: i64,
    pub(crate) id: Uuid,
    pub(crate) library_id: Option<Uuid>,
    pub(crate) matched_book_id: Uuid,
    pub(crate) matched_book_title: String,
    pub(crate) matched_book_author: Option<String>,
    pub(crate) matched_book_format: String,
    pub(crate) source_kind: ExactFileDiscoverySource,
    pub(crate) source_path: String,
    pub(crate) file_hash: String,
    pub(crate) file_size_bytes: i64,
    pub(crate) first_seen_at: DateTime<Utc>,
    pub(crate) last_seen_at: DateTime<Utc>,
    pub(crate) seen_count: i64,
}

#[derive(Debug)]
struct RawExactFileDiscoveryRecord {
    total_count: i64,
    id: Uuid,
    library_id: Option<Uuid>,
    matched_book_id: Uuid,
    matched_book_title: String,
    matched_book_author: Option<String>,
    matched_book_format: String,
    source_kind: String,
    source_path: String,
    file_hash: String,
    file_size_bytes: i64,
    first_seen_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    seen_count: i64,
}

impl TryFrom<RawExactFileDiscoveryRecord> for ExactFileDiscoveryRecord {
    type Error = Error;

    fn try_from(row: RawExactFileDiscoveryRecord) -> Result<Self> {
        let source_kind =
            ExactFileDiscoverySource::from_str(&row.source_kind).map_err(|error| {
                Error::Internal(format!(
                    "invalid exact-file discovery source persisted in database: {error}"
                ))
            })?;

        Ok(Self {
            total_count: row.total_count,
            id: row.id,
            library_id: row.library_id,
            matched_book_id: row.matched_book_id,
            matched_book_title: row.matched_book_title,
            matched_book_author: row.matched_book_author,
            matched_book_format: row.matched_book_format,
            source_kind,
            source_path: row.source_path,
            file_hash: row.file_hash,
            file_size_bytes: row.file_size_bytes,
            first_seen_at: row.first_seen_at,
            last_seen_at: row.last_seen_at,
            seen_count: row.seen_count,
        })
    }
}

impl PgDuplicateRepository {
    /// Persist an exact-file skip as an idempotent discovery instead of
    /// discarding it into an aggregate counter.
    pub(crate) async fn record_exact_file_discovery(
        &self,
        input: RecordExactFileDiscovery<'_>,
    ) -> Result<ExactFileDiscoveryRecord> {
        let source_key = exact_file_source_key(input.source, input.source_path);
        let row = sqlx::query_as!(
            RawExactFileDiscoveryRecord,
            r#"
            WITH upserted AS (
                INSERT INTO exact_file_discoveries (
                    library_id, matched_book_id, source_kind, source_key,
                    source_path, file_hash, file_size_bytes, discovered_by
                )
                SELECT b.library_id, b.id, $2, $3, $4, $5, $6, $7
                FROM books b
                WHERE b.id = $1
                ON CONFLICT (matched_book_id, source_kind, source_key, file_hash)
                DO UPDATE SET
                    library_id = EXCLUDED.library_id,
                    source_path = EXCLUDED.source_path,
                    file_size_bytes = EXCLUDED.file_size_bytes,
                    discovered_by = EXCLUDED.discovered_by,
                    last_seen_at = NOW(),
                    seen_count = exact_file_discoveries.seen_count + 1
                RETURNING id, library_id, matched_book_id, source_kind,
                          source_path, file_hash, file_size_bytes,
                          first_seen_at, last_seen_at, seen_count
            )
            SELECT 1::bigint AS "total_count!",
                   d.id, d.library_id, d.matched_book_id,
                   b.title AS "matched_book_title!",
                   b.author AS matched_book_author,
                   b.format::text AS "matched_book_format!",
                   d.source_kind AS "source_kind!",
                   d.source_path AS "source_path!",
                   d.file_hash AS "file_hash!",
                   d.file_size_bytes, d.first_seen_at, d.last_seen_at,
                   d.seen_count
            FROM upserted d
            JOIN books b ON b.id = d.matched_book_id
            "#,
            input.matched_book_id,
            input.source.as_str(),
            source_key,
            input.source_path,
            input.file_hash,
            input.file_size_bytes,
            input.discovered_by,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound {
            entity: "matched exact-file book",
            id: input.matched_book_id.to_string(),
        })?;

        row.try_into()
    }

    pub(crate) async fn list_exact_file_discoveries(
        &self,
        filter: ExactFileDiscoveryFilter<'_>,
    ) -> Result<Vec<ExactFileDiscoveryRecord>> {
        let visible_library_ids = filter.visible_library_ids.map(<[Uuid]>::to_vec);
        let rows = sqlx::query_as!(
            RawExactFileDiscoveryRecord,
            r#"
            SELECT COUNT(*) OVER() AS "total_count!",
                   d.id, d.library_id, d.matched_book_id,
                   b.title AS "matched_book_title!",
                   b.author AS matched_book_author,
                   b.format::text AS "matched_book_format!",
                   d.source_kind AS "source_kind!",
                   d.source_path AS "source_path!",
                   d.file_hash AS "file_hash!",
                   d.file_size_bytes, d.first_seen_at, d.last_seen_at,
                   d.seen_count
            FROM exact_file_discoveries d
            JOIN books b ON b.id = d.matched_book_id
            WHERE ($1::uuid IS NULL OR d.library_id = $1)
              AND (
                $2::uuid[] IS NULL
                OR (d.library_id = ANY($2) AND b.library_id = ANY($2))
              )
            ORDER BY d.last_seen_at DESC, d.id DESC
            LIMIT $3 OFFSET $4
            "#,
            filter.library_id,
            visible_library_ids.as_deref(),
            filter.limit,
            filter.offset,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(TryInto::try_into).collect()
    }
}

fn exact_file_source_key(source: ExactFileDiscoverySource, source_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source.as_str().as_bytes());
    hasher.update([0]);
    hasher.update(source_path.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_identity_is_stable_and_source_scoped() {
        let first = exact_file_source_key(ExactFileDiscoverySource::Upload, "same.txt");
        let repeated = exact_file_source_key(ExactFileDiscoverySource::Upload, "same.txt");
        let library = exact_file_source_key(ExactFileDiscoverySource::LibraryScan, "same.txt");

        assert_eq!(first, repeated);
        assert_ne!(first, library);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn persisted_discovery_source_is_converted_to_shared_type() {
        let record = ExactFileDiscoveryRecord::try_from(raw_discovery("library_scan"))
            .expect("known discovery source should convert");

        assert_eq!(record.source_kind, ExactFileDiscoverySource::LibraryScan);
    }

    #[test]
    fn unknown_persisted_discovery_source_is_rejected() {
        let error = ExactFileDiscoveryRecord::try_from(raw_discovery("filesystem"))
            .expect_err("unknown discovery source must not be silently accepted");

        assert!(matches!(error, Error::Internal(message) if message.contains("filesystem")));
    }

    fn raw_discovery(source_kind: &str) -> RawExactFileDiscoveryRecord {
        let now = Utc::now();
        RawExactFileDiscoveryRecord {
            total_count: 1,
            id: Uuid::new_v4(),
            library_id: Some(Uuid::new_v4()),
            matched_book_id: Uuid::new_v4(),
            matched_book_title: "Known Book".to_string(),
            matched_book_author: Some("Known Author".to_string()),
            matched_book_format: "txt".to_string(),
            source_kind: source_kind.to_string(),
            source_path: "/library/known.txt".to_string(),
            file_hash: "0".repeat(64),
            file_size_bytes: 1024,
            first_seen_at: now,
            last_seen_at: now,
            seen_count: 1,
        }
    }
}
