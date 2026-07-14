-- Performance: additional indexes for common query patterns
-- Optimizes: library listing, book search, reading stats, entity lookup

-- Books: fast lookup by library + status (library page)
CREATE INDEX IF NOT EXISTS idx_books_library_status ON books(library_id, status);

-- Books: full-text search on title (fallback when Meilisearch is down)
CREATE INDEX IF NOT EXISTS idx_books_title_trgm ON books USING gin(title gin_trgm_ops);

-- Reconcile chapters/text_chunks from the older "index" column contract to
-- the chapter_index contract used by annotations, AI, and analytics routes.
ALTER TABLE chapters ADD COLUMN IF NOT EXISTS chapter_index INTEGER;
UPDATE chapters SET chapter_index = "index" WHERE chapter_index IS NULL;
ALTER TABLE chapters ALTER COLUMN chapter_index SET NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_chapters_book_chapter_index_unique ON chapters(book_id, chapter_index);

ALTER TABLE text_chunks ADD COLUMN IF NOT EXISTS chapter_index INTEGER;
UPDATE text_chunks tc
SET chapter_index = c.chapter_index
FROM chapters c
WHERE tc.chapter_id = c.id
  AND tc.chapter_index IS NULL;
UPDATE text_chunks SET chapter_index = 0 WHERE chapter_index IS NULL;
ALTER TABLE text_chunks ALTER COLUMN chapter_index SET NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_chunks_book_chapter_chunk ON text_chunks(book_id, chapter_index, chunk_index);

ALTER TABLE reading_sessions ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id) ON DELETE CASCADE;
ALTER TABLE annotations ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id) ON DELETE CASCADE;

-- Chapters: fast chapter lookup by book + index (reader)
CREATE INDEX IF NOT EXISTS idx_chapters_book_index ON chapters(book_id, chapter_index);

-- Reading sessions: user's recent activity (dashboard)
CREATE INDEX IF NOT EXISTS idx_reading_sessions_user_started ON reading_sessions(user_id, started_at DESC);

-- Entities: lookup by book (entity panel in reader)
CREATE INDEX IF NOT EXISTS idx_entities_book_type ON entities(book_id, entity_type);

-- Annotations: user's annotations per book (reader sidebar)
CREATE INDEX IF NOT EXISTS idx_annotations_user_book ON annotations(user_id, book_id, created_at DESC);

-- Series: lookup by library (library detail page)
CREATE INDEX IF NOT EXISTS idx_series_library_sort ON series(library_id, sort_name);

-- Feature flags: fast lookup by key
CREATE INDEX IF NOT EXISTS idx_feature_flags_key ON feature_flags(key);

-- Library permissions: fast permission check
CREATE INDEX IF NOT EXISTS idx_library_permissions_lib_user ON library_permissions(library_id, user_id);
