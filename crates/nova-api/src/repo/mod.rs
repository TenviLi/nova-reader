//! PostgreSQL (SQLx) implementations of the repository traits.

pub mod pg_book;
pub mod pg_chapter;
pub mod pg_duplicate;
pub(crate) mod pg_duplicate_resolution;
pub(crate) mod pg_duplicate_scan;
pub(crate) mod pg_exact_file_discovery;
pub mod pg_library;
pub mod pg_user;
