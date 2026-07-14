-- AI usage tracking: logs all API calls with token counts, cost estimation, and metadata
CREATE TABLE IF NOT EXISTS ai_usage_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    book_id UUID REFERENCES books(id) ON DELETE SET NULL,
    
    -- Request metadata
    operation TEXT NOT NULL,           -- 'chat', 'summarize', 'extract_entities', 'translate', 'batch_process', etc.
    model TEXT NOT NULL,               -- 'deepseek-chat', 'deepseek-reasoner', etc.
    provider TEXT NOT NULL DEFAULT 'deepseek', -- 'deepseek', 'openai', 'local'
    
    -- Token usage
    prompt_tokens INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    
    -- Cost (in USD cents for precision)
    cost_cents DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    
    -- Timing
    latency_ms INTEGER NOT NULL DEFAULT 0,
    
    -- Context
    request_summary TEXT,              -- Brief description of what was asked
    success BOOLEAN NOT NULL DEFAULT true,
    error_message TEXT,
    
    -- Metadata (flexible JSON for extra info)
    metadata JSONB DEFAULT '{}',
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for common queries
CREATE INDEX idx_ai_usage_user ON ai_usage_logs(user_id, created_at DESC);
CREATE INDEX idx_ai_usage_operation ON ai_usage_logs(operation, created_at DESC);
CREATE INDEX idx_ai_usage_book ON ai_usage_logs(book_id) WHERE book_id IS NOT NULL;
CREATE INDEX idx_ai_usage_date ON ai_usage_logs(created_at DESC);

-- Materialized view for daily aggregation (refresh periodically)
CREATE MATERIALIZED VIEW IF NOT EXISTS ai_usage_daily AS
SELECT
    DATE(created_at) AS date,
    operation,
    model,
    COUNT(*) AS request_count,
    SUM(prompt_tokens) AS total_prompt_tokens,
    SUM(completion_tokens) AS total_completion_tokens,
    SUM(total_tokens) AS total_tokens,
    SUM(cost_cents) AS total_cost_cents,
    AVG(latency_ms) AS avg_latency_ms,
    COUNT(*) FILTER (WHERE NOT success) AS error_count
FROM ai_usage_logs
GROUP BY DATE(created_at), operation, model;

CREATE UNIQUE INDEX idx_ai_usage_daily_pk ON ai_usage_daily(date, operation, model);

-- Entity profiles: enhanced entity tracking for character/setting management
CREATE TABLE IF NOT EXISTS entity_profiles (
    entity_id UUID PRIMARY KEY REFERENCES entities(id) ON DELETE CASCADE,
    
    -- Rich profile data (AI-extracted)
    appearance TEXT,                   -- Physical description
    personality TEXT,                  -- Character traits
    background TEXT,                   -- Backstory
    abilities TEXT,                    -- Powers/skills for fantasy novels
    motivation TEXT,                   -- Goals and drives
    arc_summary TEXT,                  -- Character development summary
    
    -- Structured data
    attributes JSONB DEFAULT '{}',     -- Key-value pairs (age, gender, faction, etc.)
    timeline JSONB DEFAULT '[]',       -- [{chapter: 1, event: "first appearance", ...}]
    
    -- AI confidence
    confidence_score DOUBLE PRECISION DEFAULT 0.0,
    last_updated_by TEXT DEFAULT 'ai', -- 'ai' or 'user'
    
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Location/setting profiles
CREATE TABLE IF NOT EXISTS setting_profiles (
    entity_id UUID PRIMARY KEY REFERENCES entities(id) ON DELETE CASCADE,
    
    -- Setting details
    geography TEXT,
    climate TEXT,
    culture TEXT,
    significance TEXT,                 -- Role in the story
    
    -- Connected entities
    inhabitants JSONB DEFAULT '[]',    -- UUIDs of characters associated
    events JSONB DEFAULT '[]',         -- Key events that happened here
    
    attributes JSONB DEFAULT '{}',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
