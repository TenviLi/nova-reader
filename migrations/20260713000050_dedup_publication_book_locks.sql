-- Serialize content/file invalidation with duplicate-result publication. The
-- publishing transaction takes the same per-book locks, rechecks its source
-- snapshots, and only then inserts fresh evidence. Canonical UUID ordering
-- prevents pair updates from deadlocking each other.

CREATE OR REPLACE FUNCTION lock_novel_dedup_books(book_ids UUID[])
RETURNS VOID
LANGUAGE plpgsql
AS $$
BEGIN
    PERFORM pg_advisory_xact_lock(
        hashtextextended('nova:dedup:book:' || locked_book_id::text, 0)
    )
    FROM (
        SELECT DISTINCT book_id AS locked_book_id
        FROM unnest(book_ids) AS book_id
        WHERE book_id IS NOT NULL
        ORDER BY book_id
    ) ordered_books;
END;
$$;

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

    PERFORM lock_novel_dedup_books(affected_book_ids);

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

CREATE OR REPLACE FUNCTION invalidate_novel_dedup_file_evidence()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.file_hash IS DISTINCT FROM NEW.file_hash THEN
        PERFORM lock_novel_dedup_books(ARRAY[NEW.id]);
        UPDATE duplicate_pairs
        SET stale = TRUE, updated_at = NOW()
        WHERE stale = FALSE
          AND (book_a_id = NEW.id OR book_b_id = NEW.id);
    END IF;
    RETURN NEW;
END;
$$;

COMMENT ON FUNCTION lock_novel_dedup_books(UUID[]) IS
    'Transaction-scoped per-book barrier shared by dedup publication and content invalidation.';
