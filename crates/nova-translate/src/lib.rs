//! # Nova Translate
//!
//! Glossary-aware translation engine that ensures terminology consistency
//! across hundreds of chapters. Uses RAG-assisted few-shot prompting to
//! inject glossary terms into translation context.

pub mod glossary;
pub mod engine;

pub use glossary::GlossaryManager;
pub use engine::TranslationEngine;

#[cfg(test)]
mod tests;
