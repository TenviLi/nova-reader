//! # Nova Core
//!
//! Core domain models, error types, and shared abstractions for the Nova Reader platform.
//! This crate defines the canonical types that flow between all other crates.

pub mod domain;
pub mod error;
pub mod id;
pub mod repo;

pub use error::{DedupError, Error, Result};
pub use id::Id;

#[cfg(test)]
mod tests;
