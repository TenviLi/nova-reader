-- UPDATE statements that assign identical chapter identity/content must not
-- stale valid evidence or discard fingerprint caches. The scheduling trigger
-- already treats these writes as no-ops; keep invalidation semantics aligned.
CREATE OR REPLACE FUNCTION invalidate_novel_dedup_evidence()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    affected_book_ids UUID[];
BEGIN
    IF TG_OP = 'UPDATE'
       AND OLD.book_id IS NOT DISTINCT FROM NEW.book_id
       AND OLD.chapter_index IS NOT DISTINCT FROM NEW.chapter_index
       AND OLD.content IS NOT DISTINCT FROM NEW.content THEN
        RETURN NEW;
    END IF;

    IF TG_OP = 'INSERT' THEN
        affected_book_ids := ARRAY[NEW.book_id];
    ELSIF TG_OP = 'DELETE' THEN
        affected_book_ids := ARRAY[OLD.book_id];
    ELSE
        affected_book_ids := ARRAY[OLD.book_id, NEW.book_id];
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM book_fingerprints
        WHERE book_id = ANY(affected_book_ids)
    ) AND NOT EXISTS (
        SELECT 1 FROM duplicate_pairs
        WHERE stale = FALSE
          AND (book_a_id = ANY(affected_book_ids) OR book_b_id = ANY(affected_book_ids))
    ) THEN
        IF TG_OP = 'DELETE' THEN
            RETURN OLD;
        END IF;
        RETURN NEW;
    END IF;

    UPDATE duplicate_pairs
    SET stale = TRUE, updated_at = NOW()
    WHERE stale = FALSE
      AND (book_a_id = ANY(affected_book_ids) OR book_b_id = ANY(affected_book_ids));

    DELETE FROM passage_fingerprints WHERE book_id = ANY(affected_book_ids);
    DELETE FROM chapter_fingerprints WHERE book_id = ANY(affected_book_ids);
    DELETE FROM book_fingerprints WHERE book_id = ANY(affected_book_ids);

    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;
