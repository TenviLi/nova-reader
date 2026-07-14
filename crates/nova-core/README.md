# nova-core

Nova Reader 的核心领域模型和共享类型定义。

## 职责

- **领域模型**: Book, Chapter, Series, Person, Entity, Library, ReadingSession 等
- **Repository traits**: 定义数据访问抽象接口 (9 个 repository interfaces)
- **错误类型**: 统一的 `Error` / `Result` 类型
- **共享配置**: 跨 crate 的通用结构体

## 目录结构

```
src/
├── lib.rs          # 公共 API 导出
├── error.rs        # Error enum (Db, NotFound, AiService, GraphDb, ...)
├── domain/
│   ├── book.rs     # Book, BookFormat, BookStatus
│   ├── chapter.rs  # Chapter, ChapterContent
│   ├── entity.rs   # Entity, EntityType, Relationship
│   ├── library.rs  # Library, LibraryConfig
│   ├── person.rs   # Person, PersonRole
│   ├── reading.rs  # ReadingSession, ReadingGoal, Bookmark
│   ├── search.rs   # SearchQuery, SearchResult, SearchMode
│   ├── series.rs   # Series, SeriesMetadata
│   └── user.rs     # User, UserRole
└── repo/
    ├── book.rs     # BookRepository trait
    ├── chapter.rs  # ChapterRepository trait
    ├── library.rs  # LibraryRepository trait
    ├── person.rs   # PersonRepository trait
    ├── reading.rs  # ReadingRepository trait
    ├── series.rs   # SeriesRepository trait
    ├── stats.rs    # StatsRepository trait
    └── user.rs     # UserRepository trait
```

## 使用方式

```rust
use nova_core::domain::book::{Book, BookFormat};
use nova_core::repo::BookRepository;
use nova_core::{Error, Result};
```
