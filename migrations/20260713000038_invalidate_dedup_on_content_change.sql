-- Duplicate evidence is a snapshot of chapter content. Hide it immediately
-- when that content changes instead of waiting for the asynchronous rescan.

CREATE INDEX IF NOT EXISTS idx_duplicate_pairs_book_a_active
    ON duplicate_pairs(book_a_id) WHERE stale = FALSE;
CREATE INDEX IF NOT EXISTS idx_duplicate_pairs_book_b_active
    ON duplicate_pairs(book_b_id) WHERE stale = FALSE;

CREATE OR REPLACE FUNCTION invalidate_novel_dedup_evidence()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    affected_book_ids UUID[];
BEGIN
    IF TG_OP = 'INSERT' THEN
        affected_book_ids := ARRAY[NEW.book_id];
    ELSIF TG_OP = 'DELETE' THEN
        affected_book_ids := ARRAY[OLD.book_id];
    ELSE
        affected_book_ids := ARRAY[OLD.book_id, NEW.book_id];
    END IF;

    -- Most chapter inserts belong to a brand-new import and have no cached
    -- evidence yet. Avoid repeated cleanup work for that common path.
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

    -- Removing the cache makes the next incremental/full scan recompute the
    -- hashes from source chapter text before publishing replacement evidence.
    DELETE FROM passage_fingerprints WHERE book_id = ANY(affected_book_ids);
    DELETE FROM chapter_fingerprints WHERE book_id = ANY(affected_book_ids);
    DELETE FROM book_fingerprints WHERE book_id = ANY(affected_book_ids);

    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_invalidate_novel_dedup_evidence ON chapters;
CREATE TRIGGER trg_invalidate_novel_dedup_evidence
AFTER INSERT OR DELETE OR UPDATE OF book_id, chapter_index, content ON chapters
FOR EACH ROW
EXECUTE FUNCTION invalidate_novel_dedup_evidence();

-- Raw-file equality is also persisted as pair evidence. A replaced file may
-- still parse to the same chapters, but it is no longer an exact-file match.
CREATE OR REPLACE FUNCTION invalidate_novel_dedup_file_evidence()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.file_hash IS DISTINCT FROM NEW.file_hash THEN
        UPDATE duplicate_pairs
        SET stale = TRUE, updated_at = NOW()
        WHERE stale = FALSE
          AND (book_a_id = NEW.id OR book_b_id = NEW.id);
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_invalidate_novel_dedup_file_evidence ON books;
CREATE TRIGGER trg_invalidate_novel_dedup_file_evidence
AFTER UPDATE OF file_hash ON books
FOR EACH ROW
EXECUTE FUNCTION invalidate_novel_dedup_file_evidence();
