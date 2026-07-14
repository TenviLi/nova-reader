-- User settings (JSON-blob per user for single-user mode)
CREATE TABLE IF NOT EXISTS user_settings (
    user_id UUID PRIMARY KEY DEFAULT '00000000-0000-0000-0000-000000000000',
    data JSONB NOT NULL DEFAULT '{}',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Reading activities log for timeline and heatmap
CREATE TABLE IF NOT EXISTS reading_activities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    book_id UUID REFERENCES books(id) ON DELETE CASCADE,
    activity_type TEXT NOT NULL CHECK (activity_type IN ('reading', 'annotation', 'completion', 'import')),
    description TEXT NOT NULL DEFAULT '',
    chapter_index INTEGER,
    duration_minutes INTEGER,
    pages_read INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_reading_activities_user_date ON reading_activities(user_id, created_at DESC);
CREATE INDEX idx_reading_activities_book ON reading_activities(book_id);

-- Reading sessions (for heatmap data)
CREATE TABLE IF NOT EXISTS reading_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    book_id UUID REFERENCES books(id) ON DELETE CASCADE,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ,
    duration_seconds INTEGER NOT NULL DEFAULT 0,
    pages_read INTEGER NOT NULL DEFAULT 0,
    chapter_start INTEGER,
    chapter_end INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_reading_sessions_user_date ON reading_sessions(user_id, start_time DESC);
