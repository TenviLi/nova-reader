-- Add compute_hashes column to libraries
ALTER TABLE libraries ADD COLUMN IF NOT EXISTS compute_hashes BOOLEAN NOT NULL DEFAULT true;

-- Add file_hash column to books for SHA-256 dedup
ALTER TABLE books ADD COLUMN IF NOT EXISTS file_hash VARCHAR(64);
CREATE INDEX IF NOT EXISTS idx_books_file_hash ON books(file_hash) WHERE file_hash IS NOT NULL;

-- Add series_id to books (if not exists) for entity scoping
ALTER TABLE books ADD COLUMN IF NOT EXISTS series_id UUID REFERENCES series(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_books_series_id ON books(series_id);
