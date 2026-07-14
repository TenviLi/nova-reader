use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::user::*;
use crate::Result;

#[async_trait]
pub trait UserRepository: Send + Sync {
    /// Get user by ID.
    async fn get(&self, id: Uuid) -> Result<User>;

    /// Get user by username (for login).
    async fn get_by_username(&self, username: &str) -> Result<Option<User>>;

    /// Create the initial user (first-run setup).
    async fn create(&self, username: &str, password_hash: &str) -> Result<User>;

    /// Update user password.
    async fn update_password(&self, id: Uuid, password_hash: &str) -> Result<()>;

    /// Update user preferences.
    async fn update_preferences(&self, id: Uuid, prefs: &UserPreferences) -> Result<()>;

    /// Update last login timestamp.
    async fn touch_login(&self, id: Uuid) -> Result<()>;

    /// Store a refresh token.
    async fn store_refresh_token(&self, user_id: Uuid, token_hash: &str, expires_at: chrono::DateTime<chrono::Utc>) -> Result<()>;

    /// Validate and consume a refresh token.
    async fn validate_refresh_token(&self, token_hash: &str) -> Result<Option<Uuid>>;

    /// Revoke all refresh tokens for a user (logout everywhere).
    async fn revoke_all_tokens(&self, user_id: Uuid) -> Result<()>;
}
