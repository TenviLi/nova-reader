# nova-translate

Context-aware translation engine with glossary enforcement for terminology consistency across hundreds of chapters. Uses RAG-assisted few-shot prompting via DeepSeek LLM.

## Architecture

```
src/
├── lib.rs        — Re-exports
├── engine.rs     — TranslationEngine (LLM-based translation)
└── glossary.rs   — GlossaryManager (term consistency)
```

## Key Types

```rust
pub struct TranslationEngine {
    pub async fn translate(&self, request: &TranslationRequest) -> Result<TranslationResult>;
}

pub struct GlossaryManager {
    pub async fn get_entries_for_book(&self, book_id: &Uuid) -> Result<Vec<GlossaryEntry>>;
    pub fn format_as_context(entries: &[GlossaryEntry]) -> String;
}

// TranslationRequest: { text, source_language, target_language, book_id, style_notes }
// TranslationResult: { translated_text, detected_terms, confidence }
// GlossaryEntry: { term, translation, context, frequency }
```

## Translation Flow

1. Receive request with book_id and text
2. Load glossary entries for the book
3. Build system prompt with glossary terms injected
4. Call DeepSeek LLM with few-shot translation examples
5. Parse response, extract detected new terms
6. Return translated text + confidence score

## Key Patterns

- **Glossary-first**: Always loads glossary before building prompt
- **Few-shot prompting**: Injects 3-5 examples of correct translations
- **Style notes**: Optional tone/formality customization per request
- **Term detection**: LLM identifies new terms not in glossary (for user review)
- **Confidence scoring**: 0-1 metric for translation quality estimation
- **Batch-ready**: Architecture supports parallel paragraph translation

## Use Case

Critical for light novel / web novel series where:
- Character names must be consistent (魔法 → always "magic", not "sorcery")
- Place names, skills, items need fixed translations across 500+ chapters
- Style should match (formal narration vs casual dialogue)

## Dependencies

- **Internal**: nova-core
- **External**: reqwest (DeepSeek API), serde, serde_json

## Build & Test

```bash
cargo build -p nova-translate
cargo test -p nova-translate
```
