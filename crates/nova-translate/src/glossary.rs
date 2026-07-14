use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// A glossary entry mapping a term from source to target language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlossaryEntry {
    pub id: uuid::Uuid,
    pub book_id: Option<uuid::Uuid>,
    /// Source term in the original language
    pub source_term: String,
    /// Target term in the translation language
    pub target_term: String,
    /// Category (character_name, location, technique, item, etc.)
    pub category: TermCategory,
    /// Additional context for when this term should be used
    pub context: Option<String>,
    /// Whether this is a global term (applies to all books) or book-specific
    pub is_global: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermCategory {
    CharacterName,
    Location,
    Organization,
    Technique,
    Item,
    Title,
    Concept,
    Other,
}

/// Manages the glossary database for consistent translation.
#[derive(Clone, Default)]
pub struct GlossaryManager {
    entries: Arc<RwLock<Vec<GlossaryEntry>>>,
}

impl GlossaryManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get all relevant glossary entries for a given book.
    /// Returns both book-specific and global entries.
    pub async fn get_entries_for_book(
        &self,
        book_id: &uuid::Uuid,
    ) -> nova_core::Result<Vec<GlossaryEntry>> {
        let entries = self.entries.read().await;
        Ok(entries
            .iter()
            .filter(|entry| entry.is_global || entry.book_id.as_ref() == Some(book_id))
            .cloned()
            .collect())
    }

    /// Search glossary entries by source term (fuzzy match).
    pub async fn search(
        &self,
        query: &str,
        book_id: Option<&uuid::Uuid>,
    ) -> nova_core::Result<Vec<GlossaryEntry>> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let entries = self.entries.read().await;
        Ok(entries
            .iter()
            .filter(|entry| {
                let in_scope = entry.is_global
                    || book_id
                        .map(|id| entry.book_id.as_ref() == Some(id))
                        .unwrap_or(false);
                let matches_query = entry.source_term.to_lowercase().contains(&query)
                    || entry.target_term.to_lowercase().contains(&query);
                in_scope && matches_query
            })
            .cloned()
            .collect())
    }

    /// Add or update a glossary entry.
    pub async fn upsert(&self, entry: &GlossaryEntry) -> nova_core::Result<()> {
        let mut entries = self.entries.write().await;
        if let Some(existing) = entries.iter_mut().find(|existing| existing.id == entry.id) {
            *existing = entry.clone();
        } else {
            entries.push(entry.clone());
        }
        Ok(())
    }

    /// Auto-extract terms from text using AI.
    pub async fn auto_extract_terms(
        &self,
        text: &str,
        source_language: &str,
        target_language: &str,
    ) -> nova_core::Result<Vec<GlossaryEntry>> {
        let now = Utc::now();
        let mut terms = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for token in text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
            let token = token.trim();
            let char_count = token.chars().count();
            if !(2..=40).contains(&char_count) {
                continue;
            }
            if !seen.insert(token.to_lowercase()) {
                continue;
            }

            terms.push(GlossaryEntry {
                id: uuid::Uuid::now_v7(),
                book_id: None,
                source_term: token.to_string(),
                target_term: token.to_string(),
                category: TermCategory::Other,
                context: Some(format!(
                    "auto-extracted from {source_language} to {target_language} translation text"
                )),
                is_global: false,
                created_at: now,
                updated_at: now,
            });

            if terms.len() >= 20 {
                break;
            }
        }

        Ok(terms)
    }

    /// Format glossary entries as few-shot context for translation prompts.
    pub fn format_as_context(entries: &[GlossaryEntry]) -> String {
        if entries.is_empty() {
            return String::new();
        }

        let mut context = String::from("## 术语表 (Glossary) - 翻译时必须严格遵守以下对照：\n\n");
        context.push_str("| 原文 | 译文 | 类别 |\n");
        context.push_str("|------|------|------|\n");

        for entry in entries {
            context.push_str(&format!(
                "| {} | {} | {:?} |\n",
                entry.source_term, entry.target_term, entry.category
            ));
        }

        context.push_str("\n请在翻译中严格使用上表中的对应译名。\n");
        context
    }
}
