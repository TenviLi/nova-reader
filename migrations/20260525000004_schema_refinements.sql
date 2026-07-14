-- Nova Reader: Migration V4 — schema refinements and missing columns.

-- Add original_name to persons for non-Latin names
ALTER TABLE persons ADD COLUMN IF NOT EXISTS original_name TEXT;

-- Add unique constraint on person name for get_or_create
CREATE UNIQUE INDEX IF NOT EXISTS idx_persons_unique_name ON persons(name);

-- Add unique constraint on series folder_path for get_or_create_by_path
CREATE UNIQUE INDEX IF NOT EXISTS idx_series_unique_path ON series(folder_path);

-- Reading goals: add a label column for user display
ALTER TABLE reading_goals ADD COLUMN IF NOT EXISTS label TEXT NOT NULL DEFAULT '';

-- Ensure sort_name has a default for persons
UPDATE persons SET sort_name = name WHERE sort_name IS NULL OR sort_name = '';

-- Add index for series name search
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Bookmarks: add note field
ALTER TABLE bookmarks ADD COLUMN IF NOT EXISTS note TEXT;

-- Reading sessions: ensure index on book + time for efficient queries
CREATE INDEX IF NOT EXISTS idx_sessions_book_time ON reading_sessions(book_id, started_at DESC);
