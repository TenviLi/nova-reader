-- Scope reader-owned artifacts by user so library read access can safely create
-- progress, sessions, annotations, and bookmarks without granting asset writes.

ALTER TABLE reading_progress ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id) ON DELETE CASCADE;
ALTER TABLE reading_progress DROP CONSTRAINT IF EXISTS reading_progress_book_id_key;
CREATE UNIQUE INDEX IF NOT EXISTS idx_reading_progress_user_book_unique
    ON reading_progress(user_id, book_id);
CREATE INDEX IF NOT EXISTS idx_reading_progress_user_last_read
    ON reading_progress(user_id, last_read_at DESC);

ALTER TABLE bookmarks ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id) ON DELETE CASCADE;
CREATE INDEX IF NOT EXISTS idx_bookmarks_user_book
    ON bookmarks(user_id, book_id);
