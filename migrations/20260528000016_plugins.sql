-- Plugin system: Lua/WASM/JS extension support

CREATE TABLE IF NOT EXISTS plugins (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    manifest JSONB NOT NULL,       -- PluginManifest (name, version, hooks, permissions, etc.)
    enabled BOOLEAN NOT NULL DEFAULT true,
    config JSONB NOT NULL DEFAULT '{}',  -- User-configurable plugin settings
    installed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Plugin execution logs for debugging
CREATE TABLE IF NOT EXISTS plugin_executions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    hook VARCHAR(50) NOT NULL,
    input_payload JSONB,
    output_payload JSONB,
    duration_ms INTEGER,
    status VARCHAR(20) NOT NULL DEFAULT 'success', -- 'success', 'error', 'timeout'
    error_message TEXT,
    executed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for querying plugins by hook subscription
CREATE INDEX IF NOT EXISTS idx_plugins_hooks ON plugins USING GIN ((manifest->'hooks'));
CREATE INDEX IF NOT EXISTS idx_plugins_enabled ON plugins(enabled) WHERE enabled = true;
CREATE INDEX IF NOT EXISTS idx_plugin_executions_plugin ON plugin_executions(plugin_id, executed_at DESC);
