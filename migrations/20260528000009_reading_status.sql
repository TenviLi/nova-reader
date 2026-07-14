-- Add user-facing reading status (orthogonal to processing status)
CREATE TYPE reading_status AS ENUM ('unread', 'reading', 'completed', 'on_hold', 'dropped');

ALTER TABLE books ADD COLUMN reading_status reading_status NOT NULL DEFAULT 'unread';

-- Index for filtering by reading status
CREATE INDEX idx_books_reading_status ON books(reading_status);

-- Composite index for common queries (reading + status)
CREATE INDEX idx_books_status_reading ON books(status, reading_status);
