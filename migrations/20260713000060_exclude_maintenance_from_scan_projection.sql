-- A dedup maintenance task normally has no dedup_scan_runs row, but legacy or
-- corrupt data can retain that association. Its retry/dead-letter lifecycle
-- must never downgrade an already completed scan. Keep projecting malformed
-- scan payloads by excluding only the explicit maintenance discriminator.

CREATE OR REPLACE FUNCTION sync_dedup_scan_from_task_status()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.kind = 'deduplicate'::task_kind
       AND NEW.payload->>'operation' = 'cleanup_secondary_indexes' THEN
        RETURN NEW;
    END IF;

    UPDATE dedup_scan_runs AS scan
    SET status = CASE
            WHEN NEW.status IN ('queued'::task_status, 'retrying'::task_status)
                THEN 'queued'
            WHEN NEW.status = 'running'::task_status THEN 'running'
            WHEN NEW.status = 'completed'::task_status THEN 'completed'
            WHEN NEW.status = 'cancelled'::task_status THEN 'cancelled'
            ELSE 'failed'
        END,
        progress = CASE
            WHEN NEW.status = 'completed'::task_status THEN 100
            ELSE scan.progress
        END,
        progress_message = CASE
            WHEN NEW.status IN ('failed'::task_status, 'dead_letter'::task_status)
                THEN 'failed'
            WHEN NEW.status = 'cancelled'::task_status THEN 'cancelled'
            WHEN NEW.status IN ('queued'::task_status, 'retrying'::task_status)
                 AND NEW.error_message IS NOT NULL THEN 'retrying'
            ELSE scan.progress_message
        END,
        error_message = CASE
            WHEN NEW.status IN (
                'queued'::task_status,
                'retrying'::task_status,
                'failed'::task_status,
                'dead_letter'::task_status
            ) THEN NEW.error_message
            ELSE NULL
        END,
        started_at = CASE
            WHEN NEW.status = 'running'::task_status
                THEN COALESCE(scan.started_at, NEW.started_at, NOW())
            ELSE scan.started_at
        END,
        completed_at = CASE
            WHEN NEW.status IN (
                'completed'::task_status,
                'failed'::task_status,
                'dead_letter'::task_status,
                'cancelled'::task_status
            ) THEN COALESCE(NEW.completed_at, NOW())
            ELSE NULL
        END,
        updated_at = NOW()
    WHERE scan.task_id = NEW.id;

    RETURN NEW;
END;
$$;

-- Repair the exact drift produced by the previous trigger: a scan that had
-- already reached 100% was overwritten with the linked maintenance failure.
UPDATE dedup_scan_runs AS scan
SET status = 'completed',
    progress_message = 'completed',
    error_message = NULL,
    updated_at = NOW()
FROM tasks AS task
WHERE scan.task_id = task.id
  AND task.kind = 'deduplicate'::task_kind
  AND task.payload->>'operation' = 'cleanup_secondary_indexes'
  AND task.status IN ('failed'::task_status, 'dead_letter'::task_status)
  AND scan.status = 'failed'
  AND scan.progress = 100
  AND scan.error_message IS NOT DISTINCT FROM task.error_message;

COMMENT ON FUNCTION sync_dedup_scan_from_task_status() IS
    'Atomically projects scan task lifecycle changes while excluding secondary-index maintenance tasks.';

