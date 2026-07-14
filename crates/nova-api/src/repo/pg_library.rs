use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use nova_core::domain::library::*;
use nova_core::repo::book_repo::Paginated;
use nova_core::repo::library_repo::*;
use nova_core::{Error, Result};

pub struct PgLibraryRepository {
    pool: PgPool,
}

impl PgLibraryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LibraryRepository for PgLibraryRepository {
    async fn list(&self, filter: &LibraryFilter) -> Result<Paginated<Library>> {
        let offset = (filter.page - 1) * filter.per_page;

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM libraries")
            .fetch_one(&self.pool)
            .await?;

        let rows = sqlx::query_as::<_, LibraryRow>(
            r#"
            SELECT id, name, root_path, scan_interval_secs, auto_scan,
                   book_count, total_size_bytes, last_scan_at AS last_scanned_at, created_at, updated_at
            FROM libraries
            ORDER BY name ASC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(filter.per_page)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(Paginated {
            data: rows.into_iter().map(Into::into).collect(),
            total,
            page: filter.page,
            per_page: filter.per_page,
        })
    }

    async fn get(&self, id: Uuid) -> Result<Library> {
        let row = sqlx::query_as::<_, LibraryRow>(
            r#"
            SELECT id, name, root_path, scan_interval_secs, auto_scan,
                   book_count, total_size_bytes, last_scan_at AS last_scanned_at, created_at, updated_at
            FROM libraries WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound {
            entity: "library",
            id: id.to_string(),
        })?;

        Ok(row.into())
    }

    async fn create(&self, input: &CreateLibrary) -> Result<Library> {
        // Validate path exists on filesystem
        let path = std::path::Path::new(&input.root_path);
        if !path.exists() {
            return Err(Error::Validation(format!(
                "Library path does not exist: {}",
                input.root_path
            )));
        }
        if !path.is_dir() {
            return Err(Error::Validation(format!(
                "Library path is not a directory: {}",
                input.root_path
            )));
        }

        let id = Uuid::now_v7();
        let row = sqlx::query_as::<_, LibraryRow>(
            r#"
            INSERT INTO libraries (id, name, root_path, scan_interval_secs, auto_scan,
                                  include_extensions, exclude_patterns, description)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, name, root_path, scan_interval_secs, auto_scan,
                      book_count, total_size_bytes, last_scan_at AS last_scanned_at, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(&input.name)
        .bind(&input.root_path)
        .bind(input.scan_interval_secs)
        .bind(input.auto_scan)
        .bind(serde_json::to_value(&input.include_extensions).unwrap_or_default())
        .bind(serde_json::to_value(&input.exclude_patterns).unwrap_or_default())
        .bind(&input.description)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    async fn update(&self, id: Uuid, input: &UpdateLibrary) -> Result<Library> {
        // Dynamic update - only update provided fields
        let existing = self.get(id).await?;

        let name = input.name.as_deref().unwrap_or(&existing.name);
        let auto_scan = input.auto_scan.unwrap_or(existing.auto_scan);
        let interval = input
            .scan_interval_secs
            .unwrap_or(existing.scan_interval_secs);

        let row = sqlx::query_as::<_, LibraryRow>(
            r#"
            UPDATE libraries
            SET name = $2, auto_scan = $3, scan_interval_secs = $4, updated_at = NOW()
            WHERE id = $1
            RETURNING id, name, root_path, scan_interval_secs, auto_scan,
                      book_count, total_size_bytes, last_scan_at AS last_scanned_at, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(auto_scan)
        .bind(interval)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM libraries WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_scan_result(&self, id: Uuid, book_count: i64, total_size: i64) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE libraries
            SET book_count = $2, total_size_bytes = $3, last_scan_at = NOW(), updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(book_count)
        .bind(total_size)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_shelves(&self) -> Result<Vec<Shelf>> {
        let rows = sqlx::query_as::<_, ShelfRow>(
            "SELECT id, name, description, is_ordered, book_count, created_at FROM shelves ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_collections(&self) -> Result<Vec<Collection>> {
        let rows = sqlx::query_as::<_, CollectionRow>(
            r#"
            SELECT id, name, description, cover_path, book_count, created_at, updated_at
            FROM collections ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn create_collection(&self, name: &str, description: Option<&str>) -> Result<Collection> {
        let id = Uuid::now_v7();
        let row = sqlx::query_as::<_, CollectionRow>(
            r#"
            INSERT INTO collections (id, name, description)
            VALUES ($1, $2, $3)
            RETURNING id, name, description, cover_path, book_count, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    async fn add_to_collection(&self, collection_id: Uuid, book_ids: &[Uuid]) -> Result<()> {
        for book_id in book_ids {
            sqlx::query(
                "INSERT INTO collection_books (collection_id, book_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(collection_id)
            .bind(book_id)
            .execute(&self.pool)
            .await?;
        }

        // Update count
        sqlx::query(
            "UPDATE collections SET book_count = (SELECT COUNT(*) FROM collection_books WHERE collection_id = $1) WHERE id = $1",
        )
        .bind(collection_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn remove_from_collection(&self, collection_id: Uuid, book_ids: &[Uuid]) -> Result<()> {
        for book_id in book_ids {
            sqlx::query("DELETE FROM collection_books WHERE collection_id = $1 AND book_id = $2")
                .bind(collection_id)
                .bind(book_id)
                .execute(&self.pool)
                .await?;
        }

        sqlx::query(
            "UPDATE collections SET book_count = (SELECT COUNT(*) FROM collection_books WHERE collection_id = $1) WHERE id = $1",
        )
        .bind(collection_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct LibraryRow {
    id: Uuid,
    name: String,
    root_path: String,
    scan_interval_secs: i64,
    auto_scan: bool,
    book_count: i64,
    total_size_bytes: i64,
    last_scanned_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<LibraryRow> for Library {
    fn from(row: LibraryRow) -> Self {
        Library {
            id: nova_core::Id::from_uuid(row.id),
            name: row.name,
            root_path: row.root_path,
            scan_interval_secs: row.scan_interval_secs,
            auto_scan: row.auto_scan,
            book_count: row.book_count,
            total_size_bytes: row.total_size_bytes,
            last_scanned_at: row.last_scanned_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ShelfRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    is_ordered: bool,
    book_count: i32,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<ShelfRow> for Shelf {
    fn from(row: ShelfRow) -> Self {
        Shelf {
            id: nova_core::Id::from_uuid(row.id),
            name: row.name,
            description: row.description,
            is_ordered: row.is_ordered,
            book_count: row.book_count,
            created_at: row.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct CollectionRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    cover_path: Option<String>,
    book_count: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<CollectionRow> for Collection {
    fn from(row: CollectionRow) -> Self {
        Collection {
            id: nova_core::Id::from_uuid(row.id),
            name: row.name,
            description: row.description,
            cover_path: row.cover_path,
            book_count: row.book_count,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}
