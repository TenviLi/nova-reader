use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Id;
use super::book::BookId;

/// Marker for Person IDs.
#[derive(Debug, Clone, Copy)]
pub struct PersonMarker;
pub type PersonId = Id<PersonMarker>;

/// A real-world person associated with a book (author, translator, editor, etc.).
/// Inspired by Jellyfin's People system and Calibre's author management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    pub id: PersonId,
    /// Primary display name
    pub name: String,
    /// Sortable name (pinyin or romanized)
    pub sort_name: String,
    /// Alternate names / pen names
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Role in the literary world
    pub role: PersonRole,
    /// Brief biography
    pub biography: Option<String>,
    /// Photo/avatar path
    pub image_path: Option<String>,
    /// External links (e.g., author's homepage, Wikipedia)
    #[serde(default)]
    pub links: Vec<ExternalLink>,
    /// Number of books by this person in the library
    pub book_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// What role a person plays in relation to a book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "person_role", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum PersonRole {
    Author,
    Translator,
    Editor,
    Illustrator,
    Publisher,
    Narrator,
}

/// Link a person to a book with a specific role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookPerson {
    pub book_id: BookId,
    pub person_id: PersonId,
    pub role: PersonRole,
}

/// External link (URL + label).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalLink {
    pub label: String,
    pub url: String,
}
