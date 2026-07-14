-- dedup_scan_runs is a projection of its durable tasks row. Keep both sides
-- in the same database statement so a worker crash cannot leave a terminal
-- task paired with a permanently running scan. The trigger identifies scan
-- tasks by the foreign-key relationship, not by fallible JSON decoding; dedup
-- maintenance tasks have no scan row and are unaffected.

CREATE OR REPLACE FUNCTION sync_dedup_scan_from_task_status()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
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

DROP TRIGGER IF EXISTS trg_sync_dedup_scan_from_task_status ON tasks;
CREATE TRIGGER trg_sync_dedup_scan_from_task_status
AFTER UPDATE OF status, error_message, started_at, completed_at ON tasks
FOR EACH ROW
WHEN (
    OLD.status IS DISTINCT FROM NEW.status
    OR OLD.error_message IS DISTINCT FROM NEW.error_message
    OR OLD.started_at IS DISTINCT FROM NEW.started_at
    OR OLD.completed_at IS DISTINCT FROM NEW.completed_at
)
EXECUTE FUNCTION sync_dedup_scan_from_task_status();

-- Repair any projection drift left by a crash before this trigger existed.
UPDATE dedup_scan_runs AS scan
SET status = CASE
        WHEN task.status IN ('queued'::task_status, 'retrying'::task_status) THEN 'queued'
        WHEN task.status = 'running'::task_status THEN 'running'
        WHEN task.status = 'completed'::task_status THEN 'completed'
        WHEN task.status = 'cancelled'::task_status THEN 'cancelled'
        ELSE 'failed'
    END,
    progress = CASE
        WHEN task.status = 'completed'::task_status THEN 100
        ELSE scan.progress
    END,
    progress_message = CASE
        WHEN task.status IN ('failed'::task_status, 'dead_letter'::task_status) THEN 'failed'
        WHEN task.status = 'cancelled'::task_status THEN 'cancelled'
        WHEN task.status IN ('queued'::task_status, 'retrying'::task_status)
             AND task.error_message IS NOT NULL THEN 'retrying'
        ELSE scan.progress_message
    END,
    error_message = CASE
        WHEN task.status IN (
            'queued'::task_status,
            'retrying'::task_status,
            'failed'::task_status,
            'dead_letter'::task_status
        ) THEN task.error_message
        ELSE NULL
    END,
    completed_at = CASE
        WHEN task.status IN (
            'completed'::task_status,
            'failed'::task_status,
            'dead_letter'::task_status,
            'cancelled'::task_status
        ) THEN COALESCE(task.completed_at, NOW())
        ELSE NULL
    END,
    updated_at = NOW()
FROM tasks AS task
WHERE scan.task_id = task.id
  AND scan.status IS DISTINCT FROM CASE
        WHEN task.status IN ('queued'::task_status, 'retrying'::task_status) THEN 'queued'
        WHEN task.status = 'running'::task_status THEN 'running'
        WHEN task.status = 'completed'::task_status THEN 'completed'
        WHEN task.status = 'cancelled'::task_status THEN 'cancelled'
        ELSE 'failed'
      END;

COMMENT ON FUNCTION sync_dedup_scan_from_task_status() IS
    'Atomically projects durable task lifecycle changes onto dedup scan runs.';
