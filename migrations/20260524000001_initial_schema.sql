-- Nova Reader Database Schema
-- Migration: Initial schema creation
-- Timestamp: 20260524_000001

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS vector;

-- ═══════════════════════════════════════════════════════════════════
-- ENUMS
-- ═══════════════════════════════════════════════════════════════════

CREATE TYPE book_format AS ENUM ('txt', 'epub', 'pdf', 'doc', 'docx', 'markdown', 'html');
CREATE TYPE book_status AS ENUM ('pending', 'processing', 'ready', 'duplicate', 'failed', 'archived');
CREATE TYPE language AS ENUM ('chinese', 'english', 'japanese', 'korean', 'unknown');
CREATE TYPE task_kind AS ENUM (
    'parse_file', 'generate_embeddings', 'extract_entities',
    'deduplicate', 'translate', 'clean_content', 'library_scan',
    'generate_metadata', 'build_graph_summary'
);
CREATE TYPE task_status AS ENUM ('queued', 'running', 'completed', 'failed', 'retrying', 'cancelled', 'dead_letter');
CREATE TYPE task_priority AS ENUM ('0', '1', '2', '3'); -- low, normal, high, critical
CREATE TYPE highlight_color AS ENUM ('yellow', 'green', 'blue', 'pink', 'purple', 'orange');
CREATE TYPE term_category AS ENUM (
    'character_name', 'location', 'organization', 'technique',
    'item', 'title', 'concept', 'other'
);

-- ═══════════════════════════════════════════════════════════════════
-- CORE TABLES
-- ═══════════════════════════════════════════════════════════════════

-- User (single-user system, but designed for extensibility)
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    username VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(512) NOT NULL,
    display_name VARCHAR(255),
    avatar_url VARCHAR(1024),
    preferences JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Libraries (monitored root directories)
CREATE TABLE libraries (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL,
    root_path VARCHAR(2048) NOT NULL UNIQUE,
    scan_interval_secs BIGINT NOT NULL DEFAULT 3600,
    auto_scan BOOLEAN NOT NULL DEFAULT TRUE,
    book_count BIGINT NOT NULL DEFAULT 0,
    total_size_bytes BIGINT NOT NULL DEFAULT 0,
    last_scanned_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Books (the primary entity)
CREATE TABLE books (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    library_id UUID REFERENCES libraries(id) ON DELETE SET NULL,
    title VARCHAR(1024) NOT NULL,
    author VARCHAR(512),
    description TEXT,
    language language NOT NULL DEFAULT 'unknown',
    format book_format NOT NULL,
    status book_status NOT NULL DEFAULT 'pending',
    metadata JSONB NOT NULL DEFAULT '{}',
    file_path VARCHAR(4096) NOT NULL,
    file_hash VARCHAR(128) NOT NULL,
    file_size_bytes BIGINT NOT NULL DEFAULT 0,
    chapter_count INTEGER NOT NULL DEFAULT 0,
    word_count BIGINT NOT NULL DEFAULT 0,
    cover_path VARCHAR(2048),
    -- Series information
    series_name VARCHAR(512),
    series_volume INTEGER,
    -- Rating & user state
    user_rating SMALLINT CHECK (user_rating IS NULL OR (user_rating >= 1 AND user_rating <= 5)),
    is_favorite BOOLEAN NOT NULL DEFAULT FALSE,
    -- Processing state
    indexed_at TIMESTAMPTZ,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_books_library ON books(library_id);
CREATE INDEX idx_books_status ON books(status);
CREATE INDEX idx_books_language ON books(language);
CREATE INDEX idx_books_file_hash ON books(file_hash);
CREATE INDEX idx_books_series ON books(series_name);
CREATE INDEX idx_books_title_trgm ON books USING gin(title gin_trgm_ops);
CREATE INDEX idx_books_author_trgm ON books USING gin(author gin_trgm_ops);
CREATE INDEX idx_books_created ON books(created_at DESC);
CREATE INDEX idx_books_favorite ON books(is_favorite) WHERE is_favorite = TRUE;

-- Full-text search index for books
CREATE INDEX idx_books_fts ON books USING gin(
    to_tsvector('simple', coalesce(title, '') || ' ' || coalesce(author, '') || ' ' || coalesce(description, ''))
);

-- ═══════════════════════════════════════════════════════════════════
-- CHAPTERS & CONTENT
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE chapters (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    index INTEGER NOT NULL,
    chapter_index INTEGER NOT NULL,
    title VARCHAR(1024),
    content TEXT NOT NULL,
    word_count INTEGER NOT NULL DEFAULT 0,
    -- For content-based navigation
    start_offset BIGINT NOT NULL DEFAULT 0,
    end_offset BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(book_id, index),
    UNIQUE(book_id, chapter_index)
);

CREATE INDEX idx_chapters_book ON chapters(book_id, index);
CREATE INDEX idx_chapters_book_chapter_index ON chapters(book_id, chapter_index);

-- Text chunks for embedding/RAG
CREATE TABLE text_chunks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    chapter_id UUID NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    chapter_index INTEGER NOT NULL,
    chunk_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    token_count INTEGER NOT NULL DEFAULT 0,
    start_offset BIGINT NOT NULL DEFAULT 0,
    end_offset BIGINT NOT NULL DEFAULT 0,
    -- Embedding vector (using pgvector)
    embedding vector(1024),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_chunks_chapter ON text_chunks(chapter_id);
CREATE INDEX idx_chunks_book ON text_chunks(book_id);
CREATE UNIQUE INDEX idx_chunks_book_chapter_chunk ON text_chunks(book_id, chapter_index, chunk_index);
CREATE INDEX idx_chunks_embedding ON text_chunks USING ivfflat (embedding vector_cosine_ops)
    WITH (lists = 100);

-- ═══════════════════════════════════════════════════════════════════
-- READING & ANNOTATIONS
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE reading_progress (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    chapter_id UUID REFERENCES chapters(id) ON DELETE SET NULL,
    -- CFI for EPUB-style atomic position
    cfi VARCHAR(1024),
    progress DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    current_chapter INTEGER NOT NULL DEFAULT 0,
    scroll_position DOUBLE PRECISION,
    reading_time_secs BIGINT NOT NULL DEFAULT 0,
    last_read_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(book_id) -- One progress record per book
);

CREATE INDEX idx_progress_last_read ON reading_progress(last_read_at DESC);

CREATE TABLE annotations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    chapter_id UUID NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    cfi_range VARCHAR(2048),
    selected_text TEXT NOT NULL,
    note TEXT,
    color highlight_color NOT NULL DEFAULT 'yellow',
    start_offset BIGINT NOT NULL,
    end_offset BIGINT NOT NULL,
    -- Tags for organizing annotations
    tags TEXT[] DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_annotations_book ON annotations(book_id);
CREATE INDEX idx_annotations_chapter ON annotations(chapter_id);
CREATE INDEX idx_annotations_tags ON annotations USING gin(tags);

-- Bookmarks (distinct from reading progress)
CREATE TABLE bookmarks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    chapter_id UUID NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    title VARCHAR(512),
    cfi VARCHAR(1024),
    position_percent DOUBLE PRECISION,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_bookmarks_book ON bookmarks(book_id);

-- ═══════════════════════════════════════════════════════════════════
-- COLLECTIONS & ORGANIZATION
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE collections (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    cover_path VARCHAR(2048),
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE collection_books (
    collection_id UUID NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (collection_id, book_id)
);

-- Shelves (reading lists: "Want to Read", "Currently Reading", "Completed")
CREATE TABLE shelves (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    is_system BOOLEAN NOT NULL DEFAULT FALSE, -- System shelves can't be deleted
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE shelf_books (
    shelf_id UUID NOT NULL REFERENCES shelves(id) ON DELETE CASCADE,
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (shelf_id, book_id)
);

-- Tags (flexible tagging system)
CREATE TABLE tags (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL UNIQUE,
    color VARCHAR(7), -- hex color like #FF5733
    book_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE book_tags (
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    tag_id UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (book_id, tag_id)
);

-- ═══════════════════════════════════════════════════════════════════
-- KNOWLEDGE GRAPH & ENTITIES
-- ═══════════════════════════════════════════════════════════════════

-- Characters/Entities extracted from books
CREATE TABLE entities (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    name VARCHAR(512) NOT NULL,
    canonical_name VARCHAR(512),
    entity_type VARCHAR(64) NOT NULL, -- character, location, organization, etc.
    description TEXT,
    aliases TEXT[] DEFAULT '{}',
    attributes JSONB NOT NULL DEFAULT '{}',
    -- For character highlighting in the reader
    mention_count INTEGER NOT NULL DEFAULT 0,
    first_appearance_chapter INTEGER,
    -- Embedding for entity-level similarity search
    embedding vector(1024),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_entities_book ON entities(book_id);
CREATE INDEX idx_entities_type ON entities(entity_type);
CREATE INDEX idx_entities_name_trgm ON entities USING gin(name gin_trgm_ops);
CREATE INDEX idx_entities_aliases ON entities USING gin(aliases);
CREATE UNIQUE INDEX idx_entities_book_canonical ON entities(book_id, canonical_name) WHERE canonical_name IS NOT NULL;

-- Entity mentions (tracks where in the text each entity appears)
CREATE TABLE entity_mentions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    entity_id UUID NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    chapter_id UUID NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    mention_text VARCHAR(512) NOT NULL,
    start_offset INTEGER NOT NULL,
    end_offset INTEGER NOT NULL,
    context_snippet TEXT, -- surrounding text for context
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_mentions_entity ON entity_mentions(entity_id);
CREATE INDEX idx_mentions_chapter ON entity_mentions(chapter_id);

-- Entity relationships (stored in PostgreSQL as supplement to Neo4j)
CREATE TABLE entity_relationships (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    source_entity_id UUID NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    target_entity_id UUID NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relation_type VARCHAR(128) NOT NULL,
    description TEXT,
    weight DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    chapter_id UUID REFERENCES chapters(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_relationships_source ON entity_relationships(source_entity_id);
CREATE INDEX idx_relationships_target ON entity_relationships(target_entity_id);

-- ═══════════════════════════════════════════════════════════════════
-- TRANSLATION & GLOSSARY
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE glossary_entries (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    book_id UUID REFERENCES books(id) ON DELETE CASCADE,
    source_term VARCHAR(512) NOT NULL,
    target_term VARCHAR(512) NOT NULL,
    category term_category NOT NULL DEFAULT 'other',
    context TEXT,
    is_global BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_glossary_book ON glossary_entries(book_id);
CREATE INDEX idx_glossary_source ON glossary_entries(source_term);
CREATE INDEX idx_glossary_global ON glossary_entries(is_global) WHERE is_global = TRUE;
CREATE UNIQUE INDEX idx_glossary_unique ON glossary_entries(book_id, source_term) WHERE book_id IS NOT NULL;

-- Translation history
CREATE TABLE translations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    book_id UUID REFERENCES books(id) ON DELETE SET NULL,
    chapter_id UUID REFERENCES chapters(id) ON DELETE SET NULL,
    source_language VARCHAR(32) NOT NULL,
    target_language VARCHAR(32) NOT NULL,
    source_text TEXT NOT NULL,
    translated_text TEXT NOT NULL,
    model_used VARCHAR(128),
    token_count INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_translations_book ON translations(book_id);

-- ═══════════════════════════════════════════════════════════════════
-- TASK QUEUE (PostgreSQL-backed for persistence + Redis for speed)
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE tasks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    kind task_kind NOT NULL,
    status task_status NOT NULL DEFAULT 'queued',
    priority task_priority NOT NULL DEFAULT '1',
    payload JSONB NOT NULL DEFAULT '{}',
    result JSONB,
    error_message TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    -- Timing
    scheduled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_tasks_kind ON tasks(kind);
CREATE INDEX idx_tasks_scheduled ON tasks(scheduled_at) WHERE status = 'queued';
CREATE INDEX idx_tasks_created ON tasks(created_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- DEDUPLICATION
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE book_signatures (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    -- MinHash signature (stored as byte array for efficiency)
    minhash_signature BYTEA NOT NULL,
    -- Summary embedding for semantic dedup
    summary_embedding vector(1024),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(book_id)
);

CREATE TABLE duplicate_pairs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    book_a_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    book_b_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    similarity DOUBLE PRECISION NOT NULL,
    method VARCHAR(32) NOT NULL, -- 'minhash' or 'semantic'
    resolved BOOLEAN NOT NULL DEFAULT FALSE,
    resolution VARCHAR(32), -- 'keep_a', 'keep_b', 'merge', 'dismiss'
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(book_a_id, book_b_id)
);

-- ═══════════════════════════════════════════════════════════════════
-- READING STATS & ACTIVITY
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE reading_sessions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    started_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ,
    duration_secs INTEGER NOT NULL DEFAULT 0,
    pages_read INTEGER NOT NULL DEFAULT 0,
    chapters_read INTEGER NOT NULL DEFAULT 0,
    words_read BIGINT NOT NULL DEFAULT 0
);

CREATE INDEX idx_sessions_book ON reading_sessions(book_id);
CREATE INDEX idx_sessions_started ON reading_sessions(started_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- SYSTEM CONFIGURATION
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE system_config (
    key VARCHAR(255) PRIMARY KEY,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ═══════════════════════════════════════════════════════════════════
-- SEED DATA
-- ═══════════════════════════════════════════════════════════════════

-- Default shelves
INSERT INTO shelves (name, is_system, sort_order) VALUES
    ('想读', TRUE, 0),
    ('在读', TRUE, 1),
    ('已读', TRUE, 2),
    ('搁置', TRUE, 3);

-- Default system config
INSERT INTO system_config (key, value) VALUES
    ('theme', '"dark"'),
    ('reader.font_size', '18'),
    ('reader.font_family', '"Noto Serif SC"'),
    ('reader.line_height', '1.8'),
    ('reader.page_width', '720'),
    ('reader.highlight_entities', 'true'),
    ('ingest.auto_scan', 'true'),
    ('ingest.debounce_ms', '500'),
    ('ai.auto_extract_entities', 'true'),
    ('ai.auto_generate_summary', 'true');

-- Triggers for updated_at
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tr_books_updated BEFORE UPDATE ON books FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER tr_libraries_updated BEFORE UPDATE ON libraries FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER tr_annotations_updated BEFORE UPDATE ON annotations FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER tr_entities_updated BEFORE UPDATE ON entities FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER tr_glossary_updated BEFORE UPDATE ON glossary_entries FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER tr_collections_updated BEFORE UPDATE ON collections FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER tr_users_updated BEFORE UPDATE ON users FOR EACH ROW EXECUTE FUNCTION update_updated_at();
