-- Row-level prelocks are still too late for arbitrary multi-statement DML:
-- PostgreSQL may lock the target tuple before invoking BEFORE ROW, allowing
-- cycles through duplicate_pairs, the library enqueue advisory lock, or FK
-- checks. Enter one transaction-scoped barrier in BEFORE STATEMENT instead.
-- Fingerprint computation remains parallel and outside this barrier; only the
-- short source/publication/resolution write transactions are serialized.

CREATE OR REPLACE FUNCTION try_lock_novel_dedup_global_barrier()
RETURNS BOOLEAN
LANGUAGE SQL
VOLATILE
AS $$
    SELECT pg_try_advisory_xact_lock(
        hashtextextended('nova:dedup:global-write-barrier', 0)
    );
$$;

CREATE OR REPLACE FUNCTION lock_novel_dedup_global_barrier()
RETURNS VOID
LANGUAGE SQL
VOLATILE
AS $$
    SELECT pg_advisory_xact_lock(
        hashtextextended('nova:dedup:global-write-barrier', 0)
    );
$$;

CREATE OR REPLACE FUNCTION require_novel_dedup_global_barrier()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NOT try_lock_novel_dedup_global_barrier() THEN
        RAISE EXCEPTION 'novel dedup write barrier is busy; retry transaction'
            USING ERRCODE = '40001';
    END IF;
    RETURN NULL;
END;
$$;

DROP TRIGGER IF EXISTS trg_require_novel_dedup_chapter_statement_barrier ON chapters;
CREATE TRIGGER trg_require_novel_dedup_chapter_statement_barrier
BEFORE INSERT OR DELETE OR UPDATE OF book_id, chapter_index, content ON chapters
FOR EACH STATEMENT
EXECUTE FUNCTION require_novel_dedup_global_barrier();

DROP TRIGGER IF EXISTS trg_require_novel_dedup_book_statement_barrier ON books;
CREATE TRIGGER trg_require_novel_dedup_book_statement_barrier
BEFORE DELETE OR UPDATE OF file_hash, status, work_id ON books
FOR EACH STATEMENT
EXECUTE FUNCTION require_novel_dedup_global_barrier();

-- Archiving removes a version from the scannable corpus. Invalidate active
-- evidence and caches immediately; the resolution-only `duplicate` status is
-- intentionally excluded because it preserves its confirmed pair.
CREATE OR REPLACE FUNCTION invalidate_novel_dedup_archived_book()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status IS DISTINCT FROM NEW.status
       AND NEW.status = 'archived'::book_status THEN
        UPDATE duplicate_pairs
        SET stale = TRUE, updated_at = NOW()
        WHERE stale = FALSE
          AND (book_a_id = NEW.id OR book_b_id = NEW.id);

        DELETE FROM passage_fingerprints WHERE book_id = NEW.id;
        DELETE FROM chapter_fingerprints WHERE book_id = NEW.id;
        DELETE FROM book_fingerprints WHERE book_id = NEW.id;
    ELSIF OLD.status = 'archived'::book_status
          AND NEW.status NOT IN ('archived'::book_status, 'duplicate'::book_status) THEN
        PERFORM enqueue_targeted_novel_dedup_scan(NEW.id);
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_invalidate_novel_dedup_archived_book ON books;
CREATE TRIGGER trg_invalidate_novel_dedup_archived_book
AFTER UPDATE OF status ON books
FOR EACH ROW
EXECUTE FUNCTION invalidate_novel_dedup_archived_book();

COMMENT ON FUNCTION try_lock_novel_dedup_global_barrier() IS
    'Fail-fast transaction barrier acquired before any dedup-sensitive tuple lock.';
COMMENT ON FUNCTION lock_novel_dedup_global_barrier() IS
    'Blocking barrier for callers that invoke it as their transaction first lock.';
