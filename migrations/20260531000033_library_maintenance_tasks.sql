-- Library maintenance tasks and task-kind reconciliation for queued analysis work.

ALTER TYPE task_kind ADD VALUE IF NOT EXISTS 'deep_analysis';
ALTER TYPE task_kind ADD VALUE IF NOT EXISTS 'sentiment_arc';
ALTER TYPE task_kind ADD VALUE IF NOT EXISTS 'track_foreshadowing';
ALTER TYPE task_kind ADD VALUE IF NOT EXISTS 'semantic_tagging';
ALTER TYPE task_kind ADD VALUE IF NOT EXISTS 'assign_ontology';

ALTER TYPE task_kind ADD VALUE IF NOT EXISTS 'reindex_library';
ALTER TYPE task_kind ADD VALUE IF NOT EXISTS 'cleanup_orphan_covers';
ALTER TYPE task_kind ADD VALUE IF NOT EXISTS 'recompute_file_hashes';
