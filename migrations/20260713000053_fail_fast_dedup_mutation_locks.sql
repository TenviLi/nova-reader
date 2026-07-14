-- A BEFORE ROW trigger can run after PostgreSQL has identified/locked its
-- target tuple.  Waiting for the advisory lock there could still complete a
-- cycle with publication's later FK check.  Both sides therefore acquire the
-- same canonical locks without waiting.  SQLSTATE 40001 tells transaction
-- callers that a content mutation lost a short serialization race and may be
-- retried; durable dedup tasks already retry their side independently.

CREATE OR REPLACE FUNCTION require_novel_dedup_mutation_locks(book_ids UUID[])
RETURNS VOID
LANGUAGE plpgsql
AS $$
BEGIN
    IF NOT try_lock_novel_dedup_books(book_ids) THEN
        RAISE EXCEPTION 'novel dedup content lock is busy; retry transaction'
            USING ERRCODE = '40001';
    END IF;
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

    PERFORM require_novel_dedup_mutation_locks(affected_book_ids);

    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION lock_novel_dedup_book_before_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        PERFORM require_novel_dedup_mutation_locks(ARRAY[OLD.id]);
        RETURN OLD;
    END IF;

    IF OLD.file_hash IS DISTINCT FROM NEW.file_hash THEN
        PERFORM require_novel_dedup_mutation_locks(ARRAY[NEW.id]);
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_lock_novel_dedup_file_before_mutation ON books;
DROP TRIGGER IF EXISTS trg_lock_novel_dedup_book_before_mutation ON books;
CREATE TRIGGER trg_lock_novel_dedup_book_before_mutation
BEFORE DELETE OR UPDATE OF file_hash ON books
FOR EACH ROW
EXECUTE FUNCTION lock_novel_dedup_book_before_mutation();

COMMENT ON FUNCTION require_novel_dedup_mutation_locks(UUID[]) IS
    'Fails with serialization_failure instead of waiting in a dedup advisory-lock cycle.';
COMMENT ON FUNCTION lock_novel_dedup_book_before_mutation() IS
    'Protects file-hash changes and book cascades with the same fail-fast dedup barrier.';
