-- PostgreSQL CHECK constraints accept UNKNOWN, so the original paired-NULL
-- predicates did not reject partially populated grouping coordinates or text
-- ranges. Normalize any rows written in that state, then require each tuple to
-- be either completely absent or completely present and valid.

UPDATE duplicate_chapter_matches
SET alignment_group = NULL,
    segment_ordinal = NULL
WHERE (alignment_group IS NULL) <> (segment_ordinal IS NULL);

UPDATE duplicate_chapter_matches
SET chapter_a_start = NULL,
    chapter_a_end = NULL,
    chapter_b_start = NULL,
    chapter_b_end = NULL
WHERE (chapter_a_start IS NULL)::integer
    + (chapter_a_end IS NULL)::integer
    + (chapter_b_start IS NULL)::integer
    + (chapter_b_end IS NULL)::integer
      NOT IN (0, 4);

ALTER TABLE duplicate_chapter_matches
    DROP CONSTRAINT IF EXISTS chk_duplicate_match_alignment_coordinates;

ALTER TABLE duplicate_chapter_matches
    ADD CONSTRAINT chk_duplicate_match_alignment_coordinates
    CHECK (
        (alignment_group IS NULL AND segment_ordinal IS NULL)
        OR
        (alignment_group IS NOT NULL AND segment_ordinal IS NOT NULL
         AND alignment_group >= 0 AND segment_ordinal >= 0)
    );

ALTER TABLE duplicate_chapter_matches
    DROP CONSTRAINT IF EXISTS chk_duplicate_match_text_ranges;

ALTER TABLE duplicate_chapter_matches
    ADD CONSTRAINT chk_duplicate_match_text_ranges
    CHECK (
        (chapter_a_start IS NULL AND chapter_a_end IS NULL
         AND chapter_b_start IS NULL AND chapter_b_end IS NULL)
        OR
        (chapter_a_start IS NOT NULL AND chapter_a_end IS NOT NULL
         AND chapter_b_start IS NOT NULL AND chapter_b_end IS NOT NULL
         AND chapter_a_start >= 0 AND chapter_a_end > chapter_a_start
         AND chapter_b_start >= 0 AND chapter_b_end > chapter_b_start
         AND chapter_a_end - chapter_a_start = chapter_b_end - chapter_b_start)
    );
