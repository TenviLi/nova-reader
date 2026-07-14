-- Make every persisted duplicate scan task conform to the typed worker payload.
-- Older scan tasks predate the explicit discriminator and are otherwise
-- indistinguishable from malformed maintenance work after a restart.
UPDATE tasks AS task
SET payload = jsonb_set(task.payload, '{operation}', '"scan"'::jsonb, true)
FROM dedup_scan_runs AS scan
WHERE scan.task_id = task.id
  AND task.kind = 'deduplicate'::task_kind
  AND NOT (task.payload ? 'operation');

-- Progress messages are stable wire codes, never localized prose. Normalize
-- rows written by pre-code implementations so the typed API remains total.
UPDATE dedup_scan_runs
SET progress_message = CASE
    WHEN status = 'completed' THEN 'completed'
    WHEN status = 'failed' THEN 'failed'
    WHEN status = 'running' AND progress < 48 THEN 'fingerprinting'
    WHEN status = 'running' AND progress < 55 THEN 'candidate_generation'
    WHEN status = 'running' THEN 'verifying'
    WHEN status = 'queued' AND progress_message IN ('retrying', 'recovering')
        THEN progress_message
    ELSE NULL
END
WHERE progress_message IS NOT NULL
  AND progress_message NOT IN (
      'recovering', 'retrying', 'failed', 'fingerprinting',
      'candidate_generation', 'verifying', 'completed'
  );
