-- Sprint 8: Semantic Intelligence System
-- Semantic Tag Profiles, Trope Heatmaps, Content Radar

-- User-defined semantic tag profiles (reference vectors for similarity matching)
CREATE TABLE IF NOT EXISTS tag_profiles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,           -- e.g. "TSF变身", "皮モノ", "灵魂附身"
    description TEXT,              -- What this tag represents
    category TEXT NOT NULL DEFAULT 'custom', -- 'trope', 'emotion', 'setting', 'warning', 'custom'
    color TEXT NOT NULL DEFAULT '#6366f1', -- Hex color for UI
    icon TEXT,                     -- Lucide icon name
    -- Reference texts that define this tag's "vibe"
    reference_texts TEXT[] NOT NULL DEFAULT '{}',
    -- Average embedding vector (computed from reference_texts)
    embedding FLOAT4[] DEFAULT NULL,
    -- Threshold for matching (0.0 - 1.0)
    match_threshold FLOAT4 NOT NULL DEFAULT 0.45,
    is_warning BOOLEAN NOT NULL DEFAULT false, -- true = "毒点", false = "爽点" or neutral
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, name)
);

-- Per-book tag concentration scores
CREATE TABLE IF NOT EXISTS book_tag_scores (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    tag_profile_id UUID NOT NULL REFERENCES tag_profiles(id) ON DELETE CASCADE,
    -- Aggregate score: how strongly this book matches the tag (0.0 - 1.0)
    concentration FLOAT4 NOT NULL DEFAULT 0.0,
    -- Number of matching chunks above threshold
    match_count INTEGER NOT NULL DEFAULT 0,
    -- Total chunks analyzed
    total_chunks INTEGER NOT NULL DEFAULT 0,
    -- Peak chapter (highest concentration)
    peak_chapter INTEGER,
    peak_score FLOAT4,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(book_id, tag_profile_id)
);

-- Per-chapter tag scores (for heatmap visualization)
CREATE TABLE IF NOT EXISTS chapter_tag_scores (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    tag_profile_id UUID NOT NULL REFERENCES tag_profiles(id) ON DELETE CASCADE,
    chapter_index INTEGER NOT NULL,
    -- Score for this specific chapter (0.0 - 1.0)
    score FLOAT4 NOT NULL DEFAULT 0.0,
    -- Top matching chunk text snippet
    top_snippet TEXT,
    top_chunk_score FLOAT4,
    UNIQUE(book_id, tag_profile_id, chapter_index)
);

-- Content markers: detected signals (good/bad tropes, warnings) per chunk
CREATE TABLE IF NOT EXISTS content_markers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    tag_profile_id UUID NOT NULL REFERENCES tag_profiles(id) ON DELETE CASCADE,
    chapter_index INTEGER NOT NULL,
    chunk_index INTEGER NOT NULL DEFAULT 0,
    -- Similarity score that triggered this marker
    similarity_score FLOAT4 NOT NULL,
    -- Snippet of matching content
    content_snippet TEXT NOT NULL,
    -- Character offset in chapter for navigation
    char_offset INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Vibe bookmarks: saved "vibe" reference texts for quick search
CREATE TABLE IF NOT EXISTS vibe_bookmarks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT,
    source_text TEXT NOT NULL,
    -- Source location
    source_book_id UUID REFERENCES books(id) ON DELETE SET NULL,
    source_chapter_index INTEGER,
    -- Precomputed embedding
    embedding FLOAT4[],
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_book_tag_scores_book ON book_tag_scores(book_id);
CREATE INDEX IF NOT EXISTS idx_book_tag_scores_tag ON book_tag_scores(tag_profile_id);
CREATE INDEX IF NOT EXISTS idx_book_tag_scores_concentration ON book_tag_scores(concentration DESC);
CREATE INDEX IF NOT EXISTS idx_chapter_tag_scores_book ON chapter_tag_scores(book_id, tag_profile_id);
CREATE INDEX IF NOT EXISTS idx_chapter_tag_scores_lookup ON chapter_tag_scores(book_id, chapter_index);
CREATE INDEX IF NOT EXISTS idx_content_markers_book ON content_markers(book_id);
CREATE INDEX IF NOT EXISTS idx_content_markers_chapter ON content_markers(book_id, chapter_index);
CREATE INDEX IF NOT EXISTS idx_tag_profiles_user ON tag_profiles(user_id);
CREATE INDEX IF NOT EXISTS idx_vibe_bookmarks_user ON vibe_bookmarks(user_id);
