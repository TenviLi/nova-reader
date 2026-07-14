-- Conservative/layout normalization equality is useful alignment evidence,
-- but it is not byte-exact source identity. Search and RAG suppression may
-- hide a secondary chapter only when both current fingerprint snapshots also
-- carry the same raw-source SHA-256. Content mutation invalidation removes a
-- changed snapshot until a fresh scan publishes it again.
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
          AND secondary_fp.source_content_hash = primary_fp.source_content_hash
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
    'True only for a current byte-exact source chapter mapping on a confirmed duplicate secondary version.';
