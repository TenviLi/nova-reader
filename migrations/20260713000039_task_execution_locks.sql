-- Generic shared/exclusive resource locks for persistent tasks.
--
-- These are scheduling declarations, not PostgreSQL session locks. The queue
-- uses them to claim the oldest conflicting task first; the task handler may
-- still take a database advisory lock while it performs the protected work.

CREATE TABLE IF NOT EXISTS task_execution_locks (
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    resource_key TEXT NOT NULL,
    mode VARCHAR(16) NOT NULL,
    PRIMARY KEY (task_id, resource_key),
    CONSTRAINT chk_task_execution_locks_mode
        CHECK (mode IN ('shared', 'exclusive'))
);

CREATE INDEX IF NOT EXISTS idx_task_execution_locks_resource
    ON task_execution_locks(resource_key, task_id);

COMMENT ON TABLE task_execution_locks IS
    'Generic shared/exclusive resources used to serialize conflicting durable tasks';
