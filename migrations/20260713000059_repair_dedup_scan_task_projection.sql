-- Migration 54 installed the atomic tasks -> dedup_scan_runs projection, but
-- its one-time repair selected rows by status drift only. Re-run the complete
-- projection for every linked scan so same-status drift in lifecycle metadata
-- left by an older worker is repaired as well.

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
    started_at = CASE
        WHEN task.status = 'running'::task_status
            THEN COALESCE(task.started_at, scan.started_at, NOW())
        ELSE scan.started_at
    END,
    completed_at = CASE
        WHEN task.status IN (
            'completed'::task_status,
            'failed'::task_status,
            'dead_letter'::task_status,
            'cancelled'::task_status
        ) THEN COALESCE(task.completed_at, scan.completed_at, NOW())
        ELSE NULL
    END,
    updated_at = NOW()
FROM tasks AS task
WHERE scan.task_id = task.id;

