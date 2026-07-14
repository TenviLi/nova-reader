use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use nova_core::domain::user::*;
use nova_core::repo::user_repo::*;
use nova_core::{Error, Result};

pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn get(&self, id: Uuid) -> Result<User> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, password_hash, role::text, display_name, avatar_url, preferences, created_at, updated_at FROM users WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound {
            entity: "user",
            id: id.to_string(),
        })?;

        Ok(row.into())
    }

    async fn get_by_username(&self, username: &str) -> Result<Option<User>> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, password_hash, role::text, display_name, avatar_url, preferences, created_at, updated_at FROM users WHERE username = $1",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    async fn create(&self, username: &str, password_hash: &str) -> Result<User> {
        let id = Uuid::now_v7();
        let prefs = serde_json::to_value(&UserPreferences::default()).unwrap_or_default();

        let row = sqlx::query_as::<_, UserRow>(
            r#"
            INSERT INTO users (id, username, password_hash, preferences)
            VALUES ($1, $2, $3, $4)
            RETURNING id, username, password_hash, role::text, display_name, avatar_url, preferences, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(username)
        .bind(password_hash)
        .bind(&prefs)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    async fn update_password(&self, id: Uuid, password_hash: &str) -> Result<()> {
        sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1")
            .bind(id)
            .bind(password_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_preferences(&self, id: Uuid, prefs: &UserPreferences) -> Result<()> {
        let prefs_json = serde_json::to_value(prefs)
            .map_err(|e| Error::Internal(format!("serialize prefs: {e}")))?;

        sqlx::query("UPDATE users SET preferences = $2 WHERE id = $1")
            .bind(id)
            .bind(&prefs_json)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn touch_login(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE users SET updated_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn store_refresh_token(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn validate_refresh_token(&self, token_hash: &str) -> Result<Option<Uuid>> {
        let result: Option<(Uuid,)> = sqlx::query_as(
            "SELECT user_id FROM refresh_tokens WHERE token_hash = $1 AND revoked = false AND expires_at > NOW()",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(|(uid,)| uid))
    }

    async fn revoke_all_tokens(&self, user_id: Uuid) -> Result<()> {
        sqlx::query("UPDATE refresh_tokens SET revoked = true WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    username: String,
    password_hash: String,
    role: Option<String>,
    display_name: Option<String>,
    avatar_url: Option<String>,
    preferences: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        let preferences: UserPreferences =
            serde_json::from_value(row.preferences).unwrap_or_default();
        let role = match row.role.as_deref() {
            Some("admin") => UserRole::Admin,
            _ => UserRole::Reader,
        };
        User {
            id: nova_core::Id::from_uuid(row.id),
            username: row.username,
            password_hash: row.password_hash,
            role,
            display_name: row.display_name,
            avatar_url: row.avatar_url,
            preferences,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}
