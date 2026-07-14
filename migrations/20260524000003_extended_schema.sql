-- Nova Reader: Extended schema for rich metadata, auth, and analytics.
-- Adds: users, refresh_tokens, series, persons, reading_sessions, reading_goals, entity profiles

-- ═══════════════════════════════════════════════════════════════
-- Authentication
-- ═══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS users (
    id              UUID PRIMARY KEY,
    username        TEXT NOT NULL UNIQUE,
    password_hash   TEXT NOT NULL,
    display_name    TEXT,
    avatar_path     TEXT,
    preferences     JSONB NOT NULL DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login_at   TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS refresh_tokens (
    id          UUID PRIMARY KEY,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  TEXT NOT NULL UNIQUE,
    expires_at  TIMESTAMPTZ NOT NULL,
    revoked     BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_hash ON refresh_tokens(token_hash) WHERE revoked = FALSE;
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user ON refresh_tokens(user_id);

-- ═══════════════════════════════════════════════════════════════
-- Series (auto-detected from folder structure)
-- ═══════════════════════════════════════════════════════════════

DO $$ BEGIN
    CREATE TYPE series_status AS ENUM ('ongoing', 'completed', 'hiatus', 'cancelled', 'unknown');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE TABLE IF NOT EXISTS series (
    id              UUID PRIMARY KEY,
    library_id      UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    sort_name       TEXT NOT NULL,
    original_name   TEXT,
    alternate_names TEXT[] DEFAULT '{}',
    description     TEXT,
    folder_path     TEXT NOT NULL,
    status          series_status NOT NULL DEFAULT 'unknown',
    book_count      INTEGER NOT NULL DEFAULT 0,
    total_word_count BIGINT NOT NULL DEFAULT 0,
    cover_path      TEXT,
    metadata        JSONB NOT NULL DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_series_library ON series(library_id);
CREATE INDEX IF NOT EXISTS idx_series_name ON series USING gin(name gin_trgm_ops);
CREATE UNIQUE INDEX IF NOT EXISTS idx_series_library_path ON series(library_id, folder_path);

-- Link table: which books belong to which series (ordered)
CREATE TABLE IF NOT EXISTS series_books (
    series_id   UUID NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    book_id     UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    sort_order  DOUBLE PRECISION NOT NULL DEFAULT 0,
    volume_label TEXT,
    PRIMARY KEY (series_id, book_id)
);

CREATE INDEX IF NOT EXISTS idx_series_books_order ON series_books(series_id, sort_order);

-- ═══════════════════════════════════════════════════════════════
-- People (Authors, Translators, Editors, etc.)
-- ═══════════════════════════════════════════════════════════════

DO $$ BEGIN
    CREATE TYPE person_role AS ENUM ('author', 'translator', 'editor', 'illustrator', 'publisher', 'narrator');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE TABLE IF NOT EXISTS persons (
    id          UUID PRIMARY KEY,
    name        TEXT NOT NULL,
    sort_name   TEXT NOT NULL,
    aliases     TEXT[] DEFAULT '{}',
    role        person_role NOT NULL DEFAULT 'author',
    biography   TEXT,
    image_path  TEXT,
    links       JSONB NOT NULL DEFAULT '[]',
    book_count  INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_persons_name ON persons USING gin(name gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_persons_role ON persons(role);

-- Link table: person ↔ book with role
CREATE TABLE IF NOT EXISTS book_persons (
    book_id     UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    person_id   UUID NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
    role        person_role NOT NULL,
    PRIMARY KEY (book_id, person_id, role)
);

-- ═══════════════════════════════════════════════════════════════
-- Reading Sessions & Analytics
-- ═══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS reading_sessions (
    id              UUID PRIMARY KEY,
    book_id         UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    start_chapter   INTEGER NOT NULL DEFAULT 0,
    end_chapter     INTEGER NOT NULL DEFAULT 0,
    words_read      BIGINT NOT NULL DEFAULT 0,
    duration_secs   BIGINT NOT NULL DEFAULT 0,
    pages_read      INTEGER NOT NULL DEFAULT 0,
    device          TEXT,
    started_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_sessions_book ON reading_sessions(book_id);
CREATE INDEX IF NOT EXISTS idx_sessions_date ON reading_sessions(started_at);

-- ═══════════════════════════════════════════════════════════════
-- Reading Goals
-- ═══════════════════════════════════════════════════════════════

DO $$ BEGIN
    CREATE TYPE goal_type AS ENUM ('books_finished', 'reading_minutes', 'words_read', 'pages_read', 'daily_streak');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE goal_period AS ENUM ('daily', 'weekly', 'monthly', 'yearly', 'all_time');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE TABLE IF NOT EXISTS reading_goals (
    id          UUID PRIMARY KEY,
    goal_type   goal_type NOT NULL,
    target      BIGINT NOT NULL,
    progress    BIGINT NOT NULL DEFAULT 0,
    period      goal_period NOT NULL,
    active      BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ═══════════════════════════════════════════════════════════════
-- Enhanced Entities (for GraphRAG) - add profile column
-- ═══════════════════════════════════════════════════════════════

ALTER TABLE entities ADD COLUMN IF NOT EXISTS profile JSONB NOT NULL DEFAULT '{}';
ALTER TABLE entities ADD COLUMN IF NOT EXISTS image_path TEXT;
ALTER TABLE entities ADD COLUMN IF NOT EXISTS importance_score DOUBLE PRECISION NOT NULL DEFAULT 0.0;

-- Add library_id to books for linking to specific library
ALTER TABLE books ADD COLUMN IF NOT EXISTS library_id UUID REFERENCES libraries(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_books_library ON books(library_id);

-- ═══════════════════════════════════════════════════════════════
-- Ratings & Reviews (user can rate books)
-- ═══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS book_ratings (
    book_id     UUID PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
    rating      DOUBLE PRECISION CHECK (rating >= 0 AND rating <= 10),
    review      TEXT,
    rated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ═══════════════════════════════════════════════════════════════
-- Bookmarks (named positions within a book)
-- ═══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS bookmarks (
    id          UUID PRIMARY KEY,
    book_id     UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    chapter_id  UUID REFERENCES chapters(id) ON DELETE SET NULL,
    name        TEXT NOT NULL,
    cfi         TEXT,
    chapter_index INTEGER NOT NULL DEFAULT 0,
    scroll_position DOUBLE PRECISION,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_bookmarks_book ON bookmarks(book_id);
