-- Book version control: track editions and text diffs
-- Allows comparing different versions/sources of the same book

CREATE TABLE IF NOT EXISTS book_editions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    version_label VARCHAR(100) NOT NULL, -- e.g. "起点首发", "精校版", "出版版"
    source VARCHAR(255), -- where this edition came from
    file_hash VARCHAR(64) NOT NULL,
    file_path TEXT NOT NULL,
    chapter_count INTEGER NOT NULL DEFAULT 0,
    word_count BIGINT NOT NULL DEFAULT 0,
    is_primary BOOLEAN NOT NULL DEFAULT false,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS edition_diffs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    edition_a UUID NOT NULL REFERENCES book_editions(id) ON DELETE CASCADE,
    edition_b UUID NOT NULL REFERENCES book_editions(id) ON DELETE CASCADE,
    chapter_index INTEGER NOT NULL,
    diff_type VARCHAR(20) NOT NULL, -- 'added', 'removed', 'modified', 'reordered'
    old_text TEXT,
    new_text TEXT,
    change_summary TEXT, -- AI-generated summary of what changed
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_book_editions_book ON book_editions(book_id);
CREATE INDEX IF NOT EXISTS idx_edition_diffs_book ON edition_diffs(book_id);
CREATE INDEX IF NOT EXISTS idx_edition_diffs_editions ON edition_diffs(edition_a, edition_b);

-- Only one primary edition per book
CREATE UNIQUE INDEX IF NOT EXISTS idx_book_editions_primary
    ON book_editions(book_id)
    WHERE is_primary = true;
