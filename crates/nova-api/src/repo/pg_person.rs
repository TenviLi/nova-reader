use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use nova_core::domain::person::*;
use nova_core::repo::book_repo::Paginated;
use nova_core::repo::person_repo::{PersonFilter, PersonRepository, UpdatePerson};
use nova_core::{Error, Result};

pub struct PgPersonRepository {
    pool: PgPool,
}

impl PgPersonRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PersonRepository for PgPersonRepository {
    async fn list(&self, filter: &PersonFilter) -> Result<Paginated<Person>> {
        let offset = (filter.page - 1) * filter.per_page;

        let mut conditions = vec!["1=1".to_string()];
        if let Some(ref q) = filter.search {
            conditions.push(format!(
                "(p.name ILIKE '%{}%' OR p.sort_name ILIKE '%{}%')",
                q.replace('\'', "''"),
                q.replace('\'', "''")
            ));
        }
        if let Some(ref role) = filter.role {
            let role_str = format!("{:?}", role).to_lowercase();
            conditions.push(format!("p.role::text = '{}'", role_str));
        }
        let where_clause = conditions.join(" AND ");

        let count_query = format!("SELECT COUNT(*) FROM persons p WHERE {}", where_clause);
        let total: i64 = sqlx::query_scalar(&count_query)
            .fetch_one(&self.pool)
            .await?;

        let query_str = format!(
            r#"
            SELECT p.id, p.name, p.sort_name, p.role::text as role_text,
                   p.biography, p.image_path,
                   p.aliases, p.links,
                   (SELECT COUNT(*)::int FROM book_persons bp WHERE bp.person_id = p.id) as book_count,
                   p.created_at, p.updated_at
            FROM persons p
            WHERE {}
            ORDER BY p.name ASC
            LIMIT {} OFFSET {}
            "#,
            where_clause, filter.per_page, offset
        );

        let rows = sqlx::query_as::<_, PersonRow>(&query_str)
            .fetch_all(&self.pool)
            .await?;

        Ok(Paginated {
            data: rows.into_iter().map(Into::into).collect(),
            total,
            page: filter.page,
            per_page: filter.per_page,
        })
    }

    async fn get(&self, id: Uuid) -> Result<Person> {
        let row = sqlx::query_as::<_, PersonRow>(
            r#"
            SELECT p.id, p.name, p.sort_name, p.role::text as role_text,
                   p.biography, p.image_path,
                   p.aliases, p.links,
                   (SELECT COUNT(*)::int FROM book_persons bp WHERE bp.person_id = p.id) as book_count,
                   p.created_at, p.updated_at
            FROM persons p
            WHERE p.id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound {
            entity: "person",
            id: id.to_string(),
        })?;

        Ok(row.into())
    }

    async fn get_or_create(&self, name: &str, role: PersonRole) -> Result<Person> {
        let role_str = format!("{:?}", role).to_lowercase();
        let id = Uuid::now_v7();

        let row = sqlx::query_as::<_, PersonRow>(
            r#"
            WITH ins AS (
                INSERT INTO persons (id, name, sort_name, role)
                VALUES ($1, $2, $2, $3::person_role)
                ON CONFLICT (name, role) DO NOTHING
                RETURNING id, name, sort_name, role::text as role_text,
                          biography, image_path, aliases, links, 0::int as book_count,
                          created_at, updated_at
            )
            SELECT * FROM ins
            UNION ALL
            SELECT p.id, p.name, p.sort_name, p.role::text as role_text,
                   p.biography, p.image_path, p.aliases, p.links,
                   (SELECT COUNT(*)::int FROM book_persons bp WHERE bp.person_id = p.id) as book_count,
                   p.created_at, p.updated_at
            FROM persons p
            WHERE p.name = $2 AND p.role = $3::person_role
            LIMIT 1
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(&role_str)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    async fn update(&self, id: Uuid, input: &UpdatePerson) -> Result<Person> {
        let mut sets = vec!["updated_at = NOW()".to_string()];
        if let Some(ref name) = input.name {
            sets.push(format!("name = '{}'", name.replace('\'', "''")));
        }
        if let Some(ref sort_name) = input.sort_name {
            sets.push(format!("sort_name = '{}'", sort_name.replace('\'', "''")));
        }
        if let Some(ref bio) = input.biography {
            sets.push(format!("biography = '{}'", bio.replace('\'', "''")));
        }
        if let Some(ref img) = input.image_path {
            sets.push(format!("image_path = '{}'", img.replace('\'', "''")));
        }
        if let Some(ref aliases) = input.aliases {
            let json = serde_json::to_string(aliases).unwrap_or_default();
            sets.push(format!("aliases = '{}'::jsonb", json));
        }
        if let Some(ref links) = input.links {
            let json = serde_json::to_string(links).unwrap_or_default();
            sets.push(format!("links = '{}'::jsonb", json));
        }

        let query_str = format!("UPDATE persons SET {} WHERE id = $1", sets.join(", "));
        sqlx::query(&query_str).bind(id).execute(&self.pool).await?;

        self.get(id).await
    }

    async fn link_to_book(&self, person_id: Uuid, book_id: Uuid, role: PersonRole) -> Result<()> {
        let role_str = format!("{:?}", role).to_lowercase();
        sqlx::query(
            r#"
            INSERT INTO book_persons (book_id, person_id, role)
            VALUES ($1, $2, $3::person_role)
            ON CONFLICT (book_id, person_id, role) DO NOTHING
            "#,
        )
        .bind(book_id)
        .bind(person_id)
        .bind(&role_str)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn unlink_from_book(&self, person_id: Uuid, book_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM book_persons WHERE book_id = $1 AND person_id = $2")
            .bind(book_id)
            .bind(person_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn for_book(&self, book_id: Uuid) -> Result<Vec<BookPerson>> {
        let rows: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
            r#"
            SELECT bp.book_id, bp.person_id, bp.role::text
            FROM book_persons bp
            WHERE bp.book_id = $1
            ORDER BY bp.role
            "#,
        )
        .bind(book_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(bid, pid, role_str)| BookPerson {
                book_id: nova_core::Id::from_uuid(bid),
                person_id: nova_core::Id::from_uuid(pid),
                role: parse_role(&role_str),
            })
            .collect())
    }

    async fn books(&self, person_id: Uuid) -> Result<Vec<Uuid>> {
        let rows: Vec<Uuid> =
            sqlx::query_scalar("SELECT DISTINCT book_id FROM book_persons WHERE person_id = $1")
                .bind(person_id)
                .fetch_all(&self.pool)
                .await?;

        Ok(rows)
    }

    async fn merge(&self, keep_id: Uuid, remove_id: Uuid) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // Re-assign all book links from remove_id to keep_id
        sqlx::query(
            r#"
            UPDATE book_persons SET person_id = $1
            WHERE person_id = $2
            AND NOT EXISTS (
                SELECT 1 FROM book_persons bp2
                WHERE bp2.person_id = $1 AND bp2.book_id = book_persons.book_id AND bp2.role = book_persons.role
            )
            "#,
        )
        .bind(keep_id)
        .bind(remove_id)
        .execute(&mut *tx)
        .await?;

        // Delete remaining (duplicate) links
        sqlx::query("DELETE FROM book_persons WHERE person_id = $1")
            .bind(remove_id)
            .execute(&mut *tx)
            .await?;

        // Delete the merged person
        sqlx::query("DELETE FROM persons WHERE id = $1")
            .bind(remove_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }
}

fn parse_role(s: &str) -> PersonRole {
    match s {
        "author" => PersonRole::Author,
        "translator" => PersonRole::Translator,
        "editor" => PersonRole::Editor,
        "illustrator" => PersonRole::Illustrator,
        "publisher" => PersonRole::Publisher,
        "narrator" => PersonRole::Narrator,
        _ => PersonRole::Author,
    }
}

// ─── Row mapping ────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct PersonRow {
    id: Uuid,
    name: String,
    sort_name: String,
    role_text: Option<String>,
    biography: Option<String>,
    image_path: Option<String>,
    aliases: Option<serde_json::Value>,
    links: Option<serde_json::Value>,
    book_count: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<PersonRow> for Person {
    fn from(row: PersonRow) -> Self {
        let aliases: Vec<String> = row
            .aliases
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        let links: Vec<ExternalLink> = row
            .links
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        Person {
            id: nova_core::Id::from_uuid(row.id),
            name: row.name,
            sort_name: row.sort_name,
            aliases,
            role: parse_role(row.role_text.as_deref().unwrap_or("author")),
            biography: row.biography,
            image_path: row.image_path,
            links,
            book_count: row.book_count,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}
