# nova-core

Canonical domain models and shared abstractions for the entire workspace. Defines error types, strongly-typed IDs, domain value objects, and traits used by all other crates.

## Architecture

```
src/
├── lib.rs             — Re-exports all public types
├── error.rs           — Unified Error enum + HTTP status mappings
├── id.rs              — Id<T> strongly-typed UUID wrapper
└── domain/
    ├── book.rs        — Book, BookFormat, Language
    ├── chapter.rs     — Chapter, reading progress
    ├── user.rs        — User authentication & profile
    ├── entity.rs      — Person, Place, Organization (graph nodes)
    ├── search.rs      — SearchQuery, SearchResult, SearchMode
    ├── task.rs        — TaskKind, TaskPriority, TaskStatus
    ├── annotation.rs  — Annotations, bookmarks
    ├── collection.rs  — Collections, shelves
    ├── glossary.rs    — Translation glossary entries
    ├── stats.rs       — Reading statistics, sessions
    ├── settings.rs    — User/system settings
    └── social.rs      — Friends, challenges, shared shelves
```

## Key Types

```rust
pub enum Error { Database, Redis, NotFound, Unauthorized, Forbidden, AiService, Parse, Validation, ... }
pub type Result<T> = std::result::Result<T, Error>;
pub struct Id<T>(Uuid, PhantomData<T>);  // Prevents mixing book/user IDs

pub struct Book { id, title, author, format, language, file_hash, ... }
pub enum BookFormat { Epub, Pdf, Txt, Docx, Html, Markdown }
pub enum SearchMode { Keyword, Semantic, Hybrid, Graph }
pub enum TaskKind { ParseFile, ExtractEntities, TranslateChapter, EmbedChunks, ScanLibrary }
```

## Key Patterns

- **Domain-driven design**: Models represent business concepts, not DB tables
- **Type safety**: `Id<Book>` and `Id<User>` are incompatible at compile time
- **Error composition**: Each variant maps to HTTP status + retryable flag
- **Serde-friendly**: All types derive `Serialize + Deserialize`
- **No external service deps**: Pure domain logic, no I/O

## Dependencies

- serde, serde_json, uuid, chrono, thiserror, sqlx (type macros only)

## Build & Test

```bash
cargo build -p nova-core
cargo test -p nova-core
```
