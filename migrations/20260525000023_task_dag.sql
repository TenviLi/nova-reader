-- ═══════════════════════════════════════════════════════════════════
-- Task DAG: Add dependency tracking and categorization
-- ═══════════════════════════════════════════════════════════════════

-- Add new task kinds for indexing operations
ALTER TYPE task_kind ADD VALUE IF NOT EXISTS 'index_meilisearch';
ALTER TYPE task_kind ADD VALUE IF NOT EXISTS 'sync_neo4j';
ALTER TYPE task_kind ADD VALUE IF NOT EXISTS 'compute_book_embedding';
ALTER TYPE task_kind ADD VALUE IF NOT EXISTS 'detect_communities';

-- Add DAG-related columns to tasks table
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS book_id UUID REFERENCES books(id) ON DELETE CASCADE;
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS category VARCHAR(32) NOT NULL DEFAULT 'ai';
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS progress SMALLINT NOT NULL DEFAULT 0; -- 0-100
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS progress_message TEXT;

-- Task dependency table (DAG edges)
CREATE TABLE IF NOT EXISTS task_dependencies (
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    depends_on UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, depends_on)
);

-- Indexes for efficient DAG traversal
CREATE INDEX IF NOT EXISTS idx_tasks_book_id ON tasks(book_id);
CREATE INDEX IF NOT EXISTS idx_tasks_category ON tasks(category);
CREATE INDEX IF NOT EXISTS idx_tasks_status_priority ON tasks(status, priority DESC) WHERE status = 'queued';
CREATE INDEX IF NOT EXISTS idx_task_deps_depends_on ON task_dependencies(depends_on);

-- Categories: 'import', 'preprocess', 'ai', 'index'
COMMENT ON COLUMN tasks.category IS 'Task category for UI grouping: import, preprocess, ai, index';
COMMENT ON COLUMN tasks.progress IS 'Progress percentage 0-100';
