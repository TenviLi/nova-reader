-- Library-level feature toggles (AI, translation, graph, guest access).
-- Stored as a JSONB blob so new toggles don't require schema changes.
ALTER TABLE libraries ADD COLUMN IF NOT EXISTS features JSONB NOT NULL DEFAULT '{}'::jsonb;
