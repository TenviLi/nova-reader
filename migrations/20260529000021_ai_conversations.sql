-- AI conversation memory (persistent beyond Redis TTL)
CREATE TABLE IF NOT EXISTS ai_conversations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    book_id UUID REFERENCES books(id) ON DELETE SET NULL,
    title TEXT NOT NULL DEFAULT '新对话',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS ai_conversation_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id UUID NOT NULL REFERENCES ai_conversations(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('system', 'user', 'assistant')),
    content TEXT NOT NULL,
    token_count INT DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_conversations_user ON ai_conversations(user_id, updated_at DESC);
CREATE INDEX idx_ai_conv_messages_conv ON ai_conversation_messages(conversation_id, created_at);

-- Feature flags table (alternative to storing in user_settings JSON)
-- This is more queryable and audit-friendly
CREATE TABLE IF NOT EXISTS feature_flags (
    key TEXT PRIMARY KEY,
    enabled BOOLEAN NOT NULL DEFAULT true,
    description TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by UUID REFERENCES users(id)
);

-- Insert default feature flags
INSERT INTO feature_flags (key, enabled, description) VALUES
    ('ai_chat', true, 'AI 对话功能'),
    ('ai_entities', true, 'AI 实体提取'),
    ('ai_summarize', true, 'AI 摘要生成'),
    ('ai_translate', true, 'AI 翻译'),
    ('ai_style_analysis', true, 'AI 风格分析'),
    ('ai_batch_process', true, 'AI 批量处理'),
    ('semantic_search', true, '语义向量搜索'),
    ('knowledge_graph', true, '知识图谱'),
    ('reranker', true, '搜索结果重排序')
ON CONFLICT (key) DO NOTHING;
