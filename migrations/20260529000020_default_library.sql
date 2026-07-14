-- Default library for web uploads
-- This library is auto-created for users who upload books via the web interface
-- without selecting a specific library target.

-- Add is_default column if not exists
DO $$ BEGIN
    ALTER TABLE libraries ADD COLUMN IF NOT EXISTS is_default BOOLEAN NOT NULL DEFAULT false;
EXCEPTION WHEN duplicate_column THEN NULL;
END $$;

-- Ensure only one default library
CREATE UNIQUE INDEX IF NOT EXISTS idx_libraries_default ON libraries (is_default) WHERE is_default = true;

INSERT INTO libraries (id, name, root_path, description, is_default)
VALUES (
    '00000000-0000-7000-0000-000000000001',
    '默认书库',
    './data/library/default',
    '通过网页上传的书籍默认存放于此',
    true
)
ON CONFLICT DO NOTHING;
