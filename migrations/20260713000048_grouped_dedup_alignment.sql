-- Persist source-verified text segments instead of assuming every chapter
-- alignment is one-to-one. A group may span one-to-many, many-to-one, or
-- many-to-many chapter boundaries. Approximate/grouped rows remain
-- `match_type = 'winnowing'`, so cleanup and reader-asset migration continue
-- to accept only full-chapter conservative/layout matches.

ALTER TABLE duplicate_chapter_matches
    ADD COLUMN IF NOT EXISTS alignment_group INTEGER,
    ADD COLUMN IF NOT EXISTS segment_ordinal INTEGER,
    ADD COLUMN IF NOT EXISTS chapter_a_start INTEGER,
    ADD COLUMN IF NOT EXISTS chapter_a_end INTEGER,
    ADD COLUMN IF NOT EXISTS chapter_b_start INTEGER,
    ADD COLUMN IF NOT EXISTS chapter_b_end INTEGER,
    ADD COLUMN IF NOT EXISTS matched_chars INTEGER NOT NULL DEFAULT 0;

ALTER TABLE duplicate_chapter_matches
    DROP CONSTRAINT IF EXISTS duplicate_chapter_matches_pair_id_chapter_a_id_chapter_b_id_key;

DO $$ BEGIN
    ALTER TABLE duplicate_chapter_matches
        ADD CONSTRAINT chk_duplicate_match_alignment_coordinates
        CHECK (
            (alignment_group IS NULL AND segment_ordinal IS NULL)
            OR (alignment_group >= 0 AND segment_ordinal >= 0)
        );
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE duplicate_chapter_matches
        ADD CONSTRAINT chk_duplicate_match_text_ranges
        CHECK (
            (chapter_a_start IS NULL AND chapter_a_end IS NULL
             AND chapter_b_start IS NULL AND chapter_b_end IS NULL)
            OR
            (chapter_a_start >= 0 AND chapter_a_end > chapter_a_start
             AND chapter_b_start >= 0 AND chapter_b_end > chapter_b_start
             AND chapter_a_end - chapter_a_start = chapter_b_end - chapter_b_start)
        );
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE duplicate_chapter_matches
        ADD CONSTRAINT chk_duplicate_match_matched_chars
        CHECK (matched_chars >= 0);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_duplicate_chapter_matches_group_segment
    ON duplicate_chapter_matches(pair_id, alignment_group, segment_ordinal)
    WHERE alignment_group IS NOT NULL AND segment_ordinal IS NOT NULL;

COMMENT ON COLUMN duplicate_chapter_matches.alignment_group IS
    'Pair-local source-verified segment group; repeated chapter ids express changed boundaries';
COMMENT ON COLUMN duplicate_chapter_matches.chapter_a_start IS
    'Zero-based normalized-character offset within chapter A';
COMMENT ON COLUMN duplicate_chapter_matches.chapter_b_start IS
    'Zero-based normalized-character offset within chapter B';
COMMENT ON COLUMN duplicate_chapter_matches.matched_chars IS
    'Source-verified normalized characters represented by this fragment';
