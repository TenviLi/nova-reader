-- Nova Reader: Series & Enhanced Library Schema
-- Migration: Add series model and file-system scanning enhancements

-- ═══════════════════════════════════════════════════════════════════
-- ENUMS
-- ═══════════════════════════════════════════════════════════════════

CREATE TYPE series_status AS ENUM ('ongoing', 'completed', 'hiatus', 'abandoned', 'unknown');

-- ═══════════════════════════════════════════════════════════════════
-- SERIES TABLE (Komga/Kavita-style folder-based organization)
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS series (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    library_id UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    
    -- Identity
    name VARCHAR(1024) NOT NULL,
    sort_name VARCHAR(1024) NOT NULL, -- For alphabetical sorting (e.g. "三体" → "santi")
    original_name VARCHAR(1024), -- Original language title if different
    
    -- File system reference
    folder_path VARCHAR(4096) NOT NULL UNIQUE, -- Absolute path to the series folder
    
    -- Metadata (rich, like Jellyfin)
    summary TEXT,
    author VARCHAR(512),
    artist VARCHAR(512),
    publisher VARCHAR(512),
    language language NOT NULL DEFAULT 'unknown',
    year_start INTEGER, -- Year first book published
    year_end INTEGER, -- Year series concluded (NULL if ongoing)
    status series_status NOT NULL DEFAULT 'unknown',
    
    -- Classification (Jellyfin-style rich tagging)
    genres TEXT[] NOT NULL DEFAULT '{}', -- ["玄幻", "修仙", "爽文"]
    tags TEXT[] NOT NULL DEFAULT '{}', -- ["系统流", "无敌文", "重生", "穿越"]
    age_rating VARCHAR(50), -- "Everyone", "Teen", "Mature"
    content_warnings TEXT[] NOT NULL DEFAULT '{}', -- ["暴力", "黑暗"]
    
    -- Alternate identifiers
    alternate_titles TEXT[] NOT NULL DEFAULT '{}', -- Other names for this series
    external_links JSONB NOT NULL DEFAULT '{}', -- { "novelupdates": "...", "bangumi": "..." }
    
    -- Community/Personal
    user_rating SMALLINT CHECK (user_rating IS NULL OR (user_rating >= 1 AND user_rating <= 10)),
    is_favorite BOOLEAN NOT NULL DEFAULT FALSE,
    read_progress REAL NOT NULL DEFAULT 0.0, -- 0.0 to 1.0 (computed from books)
    
    -- Cover
    cover_path VARCHAR(2048), -- Path to extracted/user-set cover image
    cover_source VARCHAR(50) DEFAULT 'auto', -- 'auto' (from book), 'user', 'external'
    
    -- AI-generated metadata
    ai_summary TEXT, -- AI-generated series synopsis
    ai_themes TEXT[] NOT NULL DEFAULT '{}', -- AI-detected themes
    ai_mood VARCHAR(100), -- AI-detected tone ("dark", "lighthearted", "epic")
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_series_library ON series(library_id);
CREATE INDEX idx_series_name_trgm ON series USING gin(name gin_trgm_ops);
CREATE INDEX idx_series_genres ON series USING gin(genres);
CREATE INDEX idx_series_tags ON series USING gin(tags);
CREATE INDEX idx_series_status ON series(status);
CREATE INDEX idx_series_favorite ON series(is_favorite) WHERE is_favorite = TRUE;

-- ═══════════════════════════════════════════════════════════════════
-- UPDATE BOOKS TABLE to link to series
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE books ADD COLUMN series_id UUID REFERENCES series(id) ON DELETE SET NULL;
ALTER TABLE books ADD COLUMN volume_number REAL; -- 1, 2, 3.5 (for side stories)
ALTER TABLE books ADD COLUMN sort_number REAL; -- Explicit sort order within series
ALTER TABLE books ADD COLUMN file_size BIGINT NOT NULL DEFAULT 0;
ALTER TABLE books ADD COLUMN file_modified_at TIMESTAMPTZ;
ALTER TABLE books ADD COLUMN progress REAL NOT NULL DEFAULT 0.0;

CREATE INDEX idx_books_series_id ON books(series_id);
CREATE INDEX idx_books_volume ON books(series_id, volume_number);

-- ═══════════════════════════════════════════════════════════════════
-- ENHANCED LIBRARIES TABLE
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE libraries ADD COLUMN description TEXT;
ALTER TABLE libraries ADD COLUMN include_extensions JSONB DEFAULT '["txt","epub","pdf","docx","md","html"]';
ALTER TABLE libraries ADD COLUMN exclude_patterns JSONB DEFAULT '[]';
ALTER TABLE libraries ADD COLUMN scan_status VARCHAR(50) NOT NULL DEFAULT 'idle';
ALTER TABLE libraries ADD COLUMN last_scan_duration_ms BIGINT;
-- Rename for consistency
ALTER TABLE libraries RENAME COLUMN last_scanned_at TO last_scan_at;

-- ═══════════════════════════════════════════════════════════════════
-- CHARACTERS TABLE (Jellyfin-style people database)
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS characters (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    
    -- Identity
    name VARCHAR(512) NOT NULL,
    sort_name VARCHAR(512),
    original_name VARCHAR(512), -- Original language name
    aliases TEXT[] NOT NULL DEFAULT '{}', -- ["韩老魔", "三条", "张三"]
    
    -- Profile (like Jellyfin actor profiles)
    description TEXT, -- Full character bio
    role VARCHAR(100), -- "protagonist", "antagonist", "supporting", "mentioned"
    gender VARCHAR(50),
    species VARCHAR(100), -- "人类", "妖族", "龙族"
    affiliation VARCHAR(512), -- "逍遥派", "少林"
    
    -- Attributes (genre-specific, extensible)
    attributes JSONB NOT NULL DEFAULT '{}',
    -- Example: { "cultivation_level": "元婴期", "weapon": "诛仙剑", "bloodline": "龙族" }
    
    -- Appearance
    avatar_path VARCHAR(2048), -- Generated or user-set avatar
    
    -- Relationships stored in graph, but denormalized here for quick access
    first_appearance_book_id UUID REFERENCES books(id),
    first_appearance_chapter INTEGER,
    mention_count INTEGER NOT NULL DEFAULT 0,
    
    -- AI metadata
    ai_personality_summary TEXT, -- AI-generated personality description
    ai_arc_summary TEXT, -- AI-detected character arc
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_characters_name_trgm ON characters USING gin(name gin_trgm_ops);
CREATE INDEX idx_characters_aliases ON characters USING gin(aliases);

-- Many-to-many: characters appear in series
CREATE TABLE IF NOT EXISTS series_characters (
    series_id UUID NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    character_id UUID NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    role VARCHAR(100) DEFAULT 'supporting', -- "protagonist", "antagonist", "supporting"
    importance INTEGER NOT NULL DEFAULT 50, -- 0-100 importance score
    first_volume REAL, -- Volume they first appear
    PRIMARY KEY (series_id, character_id)
);

-- ═══════════════════════════════════════════════════════════════════
-- ENHANCED GLOSSARY (per-series terminology, like Jellyfin's metadata)
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE glossary_entries ADD COLUMN IF NOT EXISTS series_id UUID REFERENCES series(id) ON DELETE CASCADE;
ALTER TABLE glossary_entries ADD COLUMN IF NOT EXISTS pinyin VARCHAR(512);
ALTER TABLE glossary_entries ADD COLUMN IF NOT EXISTS usage_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE glossary_entries ADD COLUMN IF NOT EXISTS first_chapter INTEGER;
ALTER TABLE glossary_entries ADD COLUMN IF NOT EXISTS related_terms TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE glossary_entries ADD COLUMN IF NOT EXISTS pronunciation_guide TEXT; -- For foreign names

-- ═══════════════════════════════════════════════════════════════════
-- READING SESSIONS (detailed activity tracking)
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS reading_sessions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at TIMESTAMPTZ,
    
    -- Progress during this session
    start_chapter INTEGER NOT NULL,
    end_chapter INTEGER,
    start_scroll REAL NOT NULL DEFAULT 0,
    end_scroll REAL,
    
    -- Computed stats
    duration_seconds INTEGER,
    words_read INTEGER,
    pages_read INTEGER,
    
    -- Context
    device VARCHAR(100), -- "web", "mobile", "tablet"
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE reading_sessions ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id) ON DELETE CASCADE;
ALTER TABLE reading_sessions ADD COLUMN IF NOT EXISTS start_chapter INTEGER;
ALTER TABLE reading_sessions ADD COLUMN IF NOT EXISTS end_chapter INTEGER;
ALTER TABLE reading_sessions ADD COLUMN IF NOT EXISTS start_scroll REAL NOT NULL DEFAULT 0;
ALTER TABLE reading_sessions ADD COLUMN IF NOT EXISTS end_scroll REAL;
ALTER TABLE reading_sessions ADD COLUMN IF NOT EXISTS duration_seconds INTEGER;
ALTER TABLE reading_sessions ADD COLUMN IF NOT EXISTS device VARCHAR(100);
ALTER TABLE reading_sessions ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

CREATE INDEX IF NOT EXISTS idx_sessions_user_book ON reading_sessions(user_id, book_id);
CREATE INDEX IF NOT EXISTS idx_sessions_started ON reading_sessions(started_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- READING GOALS
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS reading_goals (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    
    goal_type VARCHAR(50) NOT NULL, -- "daily_minutes", "weekly_books", "monthly_words"
    target_value INTEGER NOT NULL,
    period_start DATE NOT NULL,
    period_end DATE NOT NULL,
    current_value INTEGER NOT NULL DEFAULT 0,
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ═══════════════════════════════════════════════════════════════════
-- FILE SIGNATURES (for detecting changes during rescans)
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS file_signatures (
    file_path VARCHAR(4096) PRIMARY KEY,
    library_id UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    book_id UUID REFERENCES books(id) ON DELETE SET NULL,
    
    file_hash_blake3 VARCHAR(64) NOT NULL, -- BLAKE3 hash for change detection
    file_size BIGINT NOT NULL,
    file_modified_at TIMESTAMPTZ NOT NULL,
    
    last_verified_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_file_signatures_library ON file_signatures(library_id);
CREATE INDEX idx_file_signatures_hash ON file_signatures(file_hash_blake3);

-- ═══════════════════════════════════════════════════════════════════
-- METADATA SIDECAR SUPPORT
-- ═══════════════════════════════════════════════════════════════════
-- series.json or metadata.json in a series folder can override detected metadata.
-- Format:
-- {
--   "name": "斗破苍穹",
--   "author": "天蚕土豆",
--   "status": "completed",
--   "summary": "...",
--   "genres": ["玄幻", "热血"],
--   "tags": ["异火", "斗气"],
--   "year": 2009,
--   "language": "chinese",
--   "external_links": { "novelupdates": "https://..." }
-- }
