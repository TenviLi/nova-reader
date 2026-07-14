-- Chapter DELETE cascades can lock fingerprint rows before an AFTER trigger
-- gets the per-book advisory lock.  Fingerprint replacement takes those locks
-- in the opposite order, so acquire the book lock in a BEFORE ROW trigger.
--
-- A statement can still visit several books in an arbitrary row order.  Scan
-- publication therefore uses the non-blocking helper below: if any canonical
-- book lock is busy, the durable scan transaction rolls back and retries
-- instead of completing the other half of a wait cycle.

CREATE OR REPLACE FUNCTION try_lock_novel_dedup_books(book_ids UUID[])
RETURNS BOOLEAN
LANGUAGE plpgsql
AS $$
DECLARE
    locked_book_id UUID;
BEGIN
    FOR locked_book_id IN
        SELECT DISTINCT book_id
        FROM unnest(book_ids) AS book_id
        WHERE book_id IS NOT NULL
        ORDER BY book_id
    LOOP
        IF NOT pg_try_advisory_xact_lock(
            hashtextextended('nova:dedup:book:' || locked_book_id::text, 0)
        ) THEN
            RETURN FALSE;
        END IF;
    END LOOP;

    RETURN TRUE;
END;
$$;

CREATE OR REPLACE FUNCTION lock_novel_dedup_chapter_before_mutation()
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

    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_lock_novel_dedup_chapter_before_mutation ON chapters;
CREATE TRIGGER trg_lock_novel_dedup_chapter_before_mutation
BEFORE INSERT OR DELETE OR UPDATE OF book_id, chapter_index, content ON chapters
FOR EACH ROW
EXECUTE FUNCTION lock_novel_dedup_chapter_before_mutation();

CREATE OR REPLACE FUNCTION lock_novel_dedup_file_before_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.file_hash IS DISTINCT FROM NEW.file_hash THEN
        PERFORM lock_novel_dedup_books(ARRAY[NEW.id]);
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_lock_novel_dedup_file_before_mutation ON books;
CREATE TRIGGER trg_lock_novel_dedup_file_before_mutation
BEFORE UPDATE OF file_hash ON books
FOR EACH ROW
EXECUTE FUNCTION lock_novel_dedup_file_before_mutation();

COMMENT ON FUNCTION try_lock_novel_dedup_books(UUID[]) IS
    'Canonically tries per-book dedup publication locks; FALSE makes durable scans roll back and retry without deadlocking content mutations.';
COMMENT ON FUNCTION lock_novel_dedup_chapter_before_mutation() IS
    'Acquires the dedup book barrier before chapter FK cascades can lock fingerprint rows.';
