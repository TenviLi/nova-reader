-- Durable idempotency keys for task-derived work and retry-safe
-- foreshadowing setup writes.

ALTER TABLE tasks
    ADD COLUMN IF NOT EXISTS idempotency_key TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_idempotency_key
    ON tasks(idempotency_key)
    WHERE idempotency_key IS NOT NULL;

-- Historical retries could insert the same setup more than once because
-- `ON CONFLICT DO NOTHING` had no matching uniqueness contract. Keep the most
-- informative row before enforcing the identity used by the worker.
WITH ranked AS (
    SELECT id,
           ROW_NUMBER() OVER (
               PARTITION BY book_id, setup_chapter,
                            COALESCE(category, 'mystery'), setup_description
               ORDER BY
                   CASE status
                       WHEN 'resolved' THEN 0
                       WHEN 'red_herring' THEN 1
                       WHEN 'dismissed' THEN 2
                       ELSE 3
                   END,
                   resolved_at DESC NULLS LAST,
                   created_at,
                   id
           ) AS duplicate_rank
    FROM foreshadowing_entries
)
DELETE FROM foreshadowing_entries AS duplicate
USING ranked
WHERE duplicate.id = ranked.id
  AND ranked.duplicate_rank > 1;

CREATE UNIQUE INDEX IF NOT EXISTS idx_foreshadowing_setup_identity
    ON foreshadowing_entries (
        book_id,
        setup_chapter,
        (COALESCE(category, 'mystery')),
        setup_description
    );

COMMENT ON COLUMN tasks.idempotency_key IS
    'Optional durable identity for task-derived work that must survive replay';
