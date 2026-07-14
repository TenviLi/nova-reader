use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Id;

/// Marker for User IDs (single user system, but designed for expansion).
#[derive(Debug, Clone, Copy)]
pub struct UserMarker;
pub type UserId = Id<UserMarker>;

/// The system user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub username: String,
    /// Argon2 hashed password
    #[serde(skip_serializing)]
    pub password_hash: String,
    /// User role for access control
    pub role: UserRole,
    /// Display name
    pub display_name: Option<String>,
    /// Avatar image URL/path
    pub avatar_url: Option<String>,
    /// User preferences (JSON blob)
    pub preferences: UserPreferences,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// User roles for multi-user permission system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "user_role", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    /// Full system control: manage users, libraries, settings
    Admin,
    /// Can read all libraries, use AI features, manage own reading progress
    Reader,
    /// Read-only access to shared libraries, no AI features
    Guest,
}

/// User preferences stored as JSON in PostgreSQL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    /// UI theme: "dark" | "light" | "system"
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Default reader font family
    pub reader_font: Option<String>,
    /// Reader font size (px)
    #[serde(default = "default_font_size")]
    pub reader_font_size: i32,
    /// Reader line height (multiplier)
    #[serde(default = "default_line_height")]
    pub reader_line_height: f64,
    /// Reader max-width (px)
    #[serde(default = "default_max_width")]
    pub reader_max_width: i32,
    /// Preferred reading language for translations
    pub preferred_language: Option<String>,
    /// Enable reading statistics tracking
    #[serde(default = "default_true")]
    pub track_reading_stats: bool,
    /// Auto-save progress interval (seconds)
    #[serde(default = "default_save_interval")]
    pub auto_save_interval_secs: i32,
    /// Default library sort order
    #[serde(default = "default_sort")]
    pub library_sort: String,
    /// Number of items per page in library view
    #[serde(default = "default_page_size")]
    pub library_page_size: i32,
    /// Enable keyboard shortcuts globally
    #[serde(default = "default_true")]
    pub keyboard_shortcuts: bool,
    /// OPDS server enabled
    #[serde(default)]
    pub opds_enabled: bool,
    /// AI features enabled
    #[serde(default = "default_true")]
    pub ai_enabled: bool,
    /// DeepSeek API key (encrypted at rest)
    #[serde(skip_serializing)]
    pub deepseek_api_key: Option<String>,
    /// Local embedding server URL
    #[serde(default = "default_embed_url")]
    pub embedding_server_url: String,
}

fn default_theme() -> String {
    "dark".into()
}
fn default_font_size() -> i32 {
    18
}
fn default_line_height() -> f64 {
    1.8
}
fn default_max_width() -> i32 {
    720
}
fn default_true() -> bool {
    true
}
fn default_save_interval() -> i32 {
    30
}
fn default_sort() -> String {
    "updated_at_desc".into()
}
fn default_page_size() -> i32 {
    24
}
fn default_embed_url() -> String {
    "http://localhost:8000".into()
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            reader_font: None,
            reader_font_size: default_font_size(),
            reader_line_height: default_line_height(),
            reader_max_width: default_max_width(),
            preferred_language: None,
            track_reading_stats: default_true(),
            auto_save_interval_secs: default_save_interval(),
            library_sort: default_sort(),
            library_page_size: default_page_size(),
            keyboard_shortcuts: default_true(),
            opds_enabled: false,
            ai_enabled: default_true(),
            deepseek_api_key: None,
            embedding_server_url: default_embed_url(),
        }
    }
}

/// JWT claims for authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// User ID
    pub sub: String,
    /// Issued at (Unix timestamp)
    pub iat: i64,
    /// Expiration (Unix timestamp)
    pub exp: i64,
    /// Token type: "access" | "refresh"
    pub token_type: String,
    /// User role (only in access tokens)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Refresh token stored in DB.
#[derive(Debug, Clone)]
pub struct RefreshToken {
    pub id: Id<RefreshTokenMarker>,
    pub user_id: UserId,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub revoked: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct RefreshTokenMarker;
