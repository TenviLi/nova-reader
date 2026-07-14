-- A targeted scan can coalesce many books. It must not inherit the lifecycle
-- of the first book that happened to trigger it: tasks.book_id uses ON DELETE
-- CASCADE, which would otherwise discard the remaining targets.

CREATE OR REPLACE FUNCTION detach_dedup_scan_task_from_single_book()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.kind = 'deduplicate'::task_kind
       AND NEW.payload->>'operation' = 'scan' THEN
        NEW.book_id := NULL;
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_detach_dedup_scan_task_from_single_book ON tasks;
CREATE TRIGGER trg_detach_dedup_scan_task_from_single_book
BEFORE INSERT OR UPDATE OF book_id, kind, payload ON tasks
FOR EACH ROW
EXECUTE FUNCTION detach_dedup_scan_task_from_single_book();

UPDATE tasks task
SET book_id = NULL
FROM dedup_scan_runs scan
WHERE scan.task_id = task.id
  AND task.book_id IS NOT NULL;

