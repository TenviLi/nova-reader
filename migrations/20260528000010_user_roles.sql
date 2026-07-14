-- Multi-user role-based access control
-- Adds user_role enum and role column to users table

DO $$ BEGIN
    CREATE TYPE user_role AS ENUM ('admin', 'reader', 'guest');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS role user_role NOT NULL DEFAULT 'reader';

-- First user created is always admin (idempotent: only if no admins exist)
UPDATE users
SET role = 'admin'
WHERE id = (SELECT id FROM users ORDER BY created_at ASC LIMIT 1)
  AND NOT EXISTS (SELECT 1 FROM users WHERE role = 'admin');

-- Per-library visibility: which users can access which libraries
CREATE TABLE IF NOT EXISTS library_permissions (
    library_id UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    can_read BOOLEAN NOT NULL DEFAULT true,
    can_write BOOLEAN NOT NULL DEFAULT false,
    can_manage BOOLEAN NOT NULL DEFAULT false,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (library_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_library_permissions_user ON library_permissions(user_id);

COMMENT ON TABLE library_permissions IS 'Per-library ACL for multi-user access control';
COMMENT ON COLUMN library_permissions.can_read IS 'User can browse and read books in this library';
COMMENT ON COLUMN library_permissions.can_write IS 'User can add/edit/delete books in this library';
COMMENT ON COLUMN library_permissions.can_manage IS 'User can manage library settings and permissions';
