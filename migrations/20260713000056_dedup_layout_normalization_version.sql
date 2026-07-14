-- A book can legitimately have zero persisted passage anchors. Counting rows
-- cannot prove which layout-normalization version produced that empty set, so
-- store the version explicitly on the book cache boundary.

ALTER TABLE book_fingerprints
    ADD COLUMN IF NOT EXISTS layout_normalization_version INTEGER NOT NULL DEFAULT 1;

COMMENT ON COLUMN book_fingerprints.layout_normalization_version IS
    'Layout normalization version used for layout_hash and passage fingerprints, including zero-anchor books.';
