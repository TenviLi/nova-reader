-- Preserve scan ordering across an upgrade when queued/running scan tasks were
-- created before task_execution_locks existed.

INSERT INTO task_execution_locks (task_id, resource_key, mode)
SELECT task_id,
       'dedup:scan:barrier',
       CASE WHEN library_id IS NULL THEN 'exclusive' ELSE 'shared' END
FROM dedup_scan_runs
ON CONFLICT (task_id, resource_key) DO NOTHING;

INSERT INTO task_execution_locks (task_id, resource_key, mode)
SELECT task_id,
       'dedup:scan:library:' || library_id::text,
       'exclusive'
FROM dedup_scan_runs
WHERE library_id IS NOT NULL
ON CONFLICT (task_id, resource_key) DO NOTHING;
