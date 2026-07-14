-- Repair cross-route schema contracts introduced during product iteration.
-- This migration is intentionally idempotent so databases that already ran a
-- patched performance migration and older databases both converge.

ALTER TABLE chapters ADD COLUMN IF NOT EXISTS chapter_index INTEGER;
UPDATE chapters SET chapter_index = "index" WHERE chapter_index IS NULL;
ALTER TABLE chapters ALTER COLUMN chapter_index SET NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_chapters_book_chapter_index_unique ON chapters(book_id, chapter_index);
CREATE INDEX IF NOT EXISTS idx_chapters_book_index ON chapters(book_id, chapter_index);

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

CREATE INDEX IF NOT EXISTS idx_reading_sessions_user_started ON reading_sessions(user_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_annotations_user_book ON annotations(user_id, book_id, created_at DESC);
