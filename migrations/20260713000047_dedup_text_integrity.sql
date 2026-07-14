-- Persist a conservative source-text integrity signal for explainable primary
-- version recommendations. Existing caches remain NULL and are recomputed by
-- the next scan because cache validation requires this field.
ALTER TABLE book_fingerprints
    ADD COLUMN IF NOT EXISTS text_integrity_bps INTEGER
        CHECK (text_integrity_bps BETWEEN 0 AND 10000);

COMMENT ON COLUMN book_fingerprints.text_integrity_bps IS
    'Visible source text free of replacement/control/mojibake markers, in basis points';
