-- Push notification settings per user

CREATE TABLE IF NOT EXISTS notification_channels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Channel type: 'discord', 'telegram', 'email', 'webpush'
    channel_type TEXT NOT NULL,
    -- Configuration (webhook URL for discord, chat_id for telegram, etc.)
    config JSONB NOT NULL DEFAULT '{}',
    -- Which events to notify about
    events TEXT[] NOT NULL DEFAULT ARRAY['book.processed', 'task.failed'],
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_notification_channels_user ON notification_channels(user_id);

COMMENT ON TABLE notification_channels IS 'Per-user notification channel configuration';
