-- Webhook/automation system for event-driven workflows

CREATE TABLE IF NOT EXISTS webhooks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    -- Event trigger: 'book.created', 'book.processed', 'chapter.translated', etc.
    event TEXT NOT NULL,
    -- Target URL or internal action
    target_type TEXT NOT NULL CHECK (target_type IN ('url', 'internal')),
    -- External URL for 'url' type
    target_url TEXT,
    -- Internal action name for 'internal' type (e.g., 'ai_pipeline', 'translate_all', 'notify')
    internal_action TEXT,
    -- JSON config (headers, body template, etc.)
    config JSONB NOT NULL DEFAULT '{}',
    -- Whether the webhook is active
    enabled BOOLEAN NOT NULL DEFAULT true,
    -- Delivery stats
    last_triggered_at TIMESTAMPTZ,
    total_triggers INTEGER NOT NULL DEFAULT 0,
    total_failures INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_webhooks_event ON webhooks(event) WHERE enabled = true;
CREATE INDEX IF NOT EXISTS idx_webhooks_user ON webhooks(user_id);

-- Webhook delivery log (for debugging failed deliveries)
CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_id UUID NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event TEXT NOT NULL,
    payload JSONB NOT NULL,
    status_code INTEGER,
    response_body TEXT,
    error TEXT,
    delivered_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Auto-prune old deliveries (keep last 100 per webhook)
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_webhook ON webhook_deliveries(webhook_id, delivered_at DESC);

COMMENT ON TABLE webhooks IS 'Event-driven automation: triggers on book/chapter lifecycle events';
COMMENT ON TABLE webhook_deliveries IS 'Delivery log for webhook debugging';
