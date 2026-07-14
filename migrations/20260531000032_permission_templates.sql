-- Reusable permission templates for library ACL grants.
-- System templates cover the common roles; admins can add custom combinations.

CREATE TABLE IF NOT EXISTS permission_templates (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    can_read BOOLEAN NOT NULL DEFAULT true,
    can_write BOOLEAN NOT NULL DEFAULT false,
    can_manage BOOLEAN NOT NULL DEFAULT false,
    is_system BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (can_write = false OR can_read = true),
    CHECK (can_manage = false OR (can_read = true AND can_write = true))
);

INSERT INTO permission_templates (id, name, description, can_read, can_write, can_manage, is_system)
VALUES
    ('00000000-0000-4000-8000-000000000101', '只读', '可浏览和阅读书库内容', true, false, false, true),
    ('00000000-0000-4000-8000-000000000102', '协作者', '可阅读并编辑书库内容', true, true, false, true),
    ('00000000-0000-4000-8000-000000000103', '管理员', '可管理书库设置和权限', true, true, true, true)
ON CONFLICT (name) DO UPDATE SET
    description = EXCLUDED.description,
    can_read = EXCLUDED.can_read,
    can_write = EXCLUDED.can_write,
    can_manage = EXCLUDED.can_manage,
    is_system = true,
    updated_at = NOW();
