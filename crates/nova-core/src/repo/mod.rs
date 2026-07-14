//! Repository trait definitions.
//! These define the data access contract without coupling to a specific DB driver.
//! Implementations live in the API crate (using SQLx).

pub mod book_repo;
pub mod chapter_repo;
pub mod entity_repo;
pub mod library_repo;
pub mod person_repo;
pub mod reading_repo;
pub mod series_repo;
pub mod stats_repo;
pub mod user_repo;
