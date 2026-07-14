use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::person::*;
use crate::Result;
use super::book_repo::Paginated;

#[derive(Debug, Clone, Default)]
pub struct PersonFilter {
    pub role: Option<PersonRole>,
    pub search: Option<String>,
    pub page: i64,
    pub per_page: i64,
}

#[async_trait]
pub trait PersonRepository: Send + Sync {
    /// List people (authors, translators, etc.) with filtering.
    async fn list(&self, filter: &PersonFilter) -> Result<Paginated<Person>>;

    /// Get a single person by ID.
    async fn get(&self, id: Uuid) -> Result<Person>;

    /// Find or create a person by name and role.
    async fn get_or_create(&self, name: &str, role: PersonRole) -> Result<Person>;

    /// Update a person's details.
    async fn update(&self, id: Uuid, input: &UpdatePerson) -> Result<Person>;

    /// Link a person to a book.
    async fn link_to_book(&self, person_id: Uuid, book_id: Uuid, role: PersonRole) -> Result<()>;

    /// Unlink a person from a book.
    async fn unlink_from_book(&self, person_id: Uuid, book_id: Uuid) -> Result<()>;

    /// Get all people associated with a book.
    async fn for_book(&self, book_id: Uuid) -> Result<Vec<BookPerson>>;

    /// Get all books by a person.
    async fn books(&self, person_id: Uuid) -> Result<Vec<Uuid>>;

    /// Merge two person records (deduplication).
    async fn merge(&self, keep_id: Uuid, remove_id: Uuid) -> Result<()>;
}

/// Input for updating a person.
#[derive(Debug, Clone, Default)]
pub struct UpdatePerson {
    pub name: Option<String>,
    pub sort_name: Option<String>,
    pub aliases: Option<Vec<String>>,
    pub biography: Option<String>,
    pub image_path: Option<String>,
    pub links: Option<Vec<ExternalLink>>,
}
