-- A resolved secondary version may still contain unique chapters. Search and
-- RAG rebuilds must omit only exact redundant mappings, never the whole book.
-- Fingerprint rows are deleted by the content-invalidation trigger, so edited
-- chapters automatically become searchable again until a fresh scan verifies
-- them.
CREATE OR REPLACE FUNCTION dedup_chapter_is_redundant(target_chapter_id UUID)
RETURNS BOOLEAN
LANGUAGE sql
STABLE
AS $$
    SELECT EXISTS (
        SELECT 1
        FROM chapters secondary_chapter
        JOIN books secondary_book
          ON secondary_book.id = secondary_chapter.book_id
        JOIN duplicate_pairs pair
          ON pair.book_a_id = secondary_chapter.book_id
          OR pair.book_b_id = secondary_chapter.book_id
        JOIN duplicate_chapter_matches mapping
          ON mapping.pair_id = pair.id
         AND secondary_chapter.id = CASE
               WHEN pair.book_a_id = secondary_chapter.book_id
                 THEN mapping.chapter_a_id
               ELSE mapping.chapter_b_id
             END
        JOIN chapters primary_chapter
          ON primary_chapter.id = CASE
               WHEN pair.book_a_id = pair.recommended_primary_id
                 THEN mapping.chapter_a_id
               ELSE mapping.chapter_b_id
             END
        JOIN chapter_fingerprints secondary_fp
          ON secondary_fp.chapter_id = secondary_chapter.id
        JOIN chapter_fingerprints primary_fp
          ON primary_fp.chapter_id = primary_chapter.id
        WHERE secondary_chapter.id = target_chapter_id
          AND secondary_book.status = 'duplicate'::book_status
          AND pair.resolved = TRUE
          AND pair.review_status = 'confirmed'
          AND pair.stale = FALSE
          AND pair.recommended_primary_id IS NOT NULL
          AND pair.recommended_primary_id <> secondary_chapter.book_id
          AND primary_chapter.book_id = pair.recommended_primary_id
          AND (
              (mapping.match_type = 'conservative'
               AND secondary_fp.conservative_hash = primary_fp.conservative_hash)
              OR
              (mapping.match_type = 'layout'
               AND secondary_fp.layout_hash = primary_fp.layout_hash)
          )
    );
$$;

COMMENT ON FUNCTION dedup_chapter_is_redundant(UUID) IS
    'True only for a source-verified exact chapter mapping on a confirmed duplicate secondary version.';
