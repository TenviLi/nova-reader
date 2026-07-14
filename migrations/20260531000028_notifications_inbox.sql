-- Real notification inbox with server-side read state.
-- Distinct from `notification_channels` (delivery config); this is the in-app feed.
CREATE TABLE IF NOT EXISTS notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    level TEXT NOT NULL DEFAULT 'info',        -- info | success | warning | error
    category TEXT NOT NULL DEFAULT 'system',   -- system | reading | ai | library | social
    title TEXT NOT NULL,
    body TEXT NOT NULL DEFAULT '',
    link TEXT,                                 -- in-app route, e.g. /library/<id>
    book_id UUID REFERENCES books(id) ON DELETE SET NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_notifications_user_created
    ON notifications (user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_notifications_user_unread
    ON notifications (user_id) WHERE read_at IS NULL;
