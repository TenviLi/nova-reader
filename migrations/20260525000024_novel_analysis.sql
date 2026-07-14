-- Novel Analysis: chapter summaries, sentiment arcs, foreshadowing tracking,
-- character state changes, and scene transitions.

-- ═══════════════════════════════════════════════════════════════════════════════
-- Chapter Summaries (micro-window condensed output)
-- ═══════════════════════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS chapter_summaries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    chapter_index INT NOT NULL,
    -- Structured summary from LLM
    summary TEXT NOT NULL,
    time_marker TEXT,               -- "深夜", "三年后", etc.
    location TEXT,                   -- primary location this chapter
    key_event TEXT,                  -- most important event
    sentiment TEXT,                  -- dominant emotion label
    sentiment_score REAL DEFAULT 0,  -- -1.0 to 1.0
    characters_present TEXT[] DEFAULT '{}',  -- who appears
    potential_mysteries TEXT[] DEFAULT '{}', -- detected foreshadowing seeds
    raw_json JSONB,                  -- full structured response from LLM
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(book_id, chapter_index)
);

-- ═══════════════════════════════════════════════════════════════════════════════
-- Sentiment Arcs (per-chapter emotion scoring)
-- ═══════════════════════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS sentiment_arcs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    chapter_index INT NOT NULL,
    -- Multi-dimensional emotion scores (0.0 - 1.0)
    joy REAL DEFAULT 0,
    sadness REAL DEFAULT 0,
    anger REAL DEFAULT 0,
    fear REAL DEFAULT 0,
    surprise REAL DEFAULT 0,
    tension REAL DEFAULT 0,
    romance REAL DEFAULT 0,
    -- Aggregate
    overall_score REAL DEFAULT 0,     -- -1.0 to 1.0
    dominant_emotion TEXT,
    is_peak BOOLEAN DEFAULT FALSE,    -- local maximum
    is_valley BOOLEAN DEFAULT FALSE,  -- local minimum
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(book_id, chapter_index)
);

-- ═══════════════════════════════════════════════════════════════════════════════
-- Foreshadowing Tracking (Setup → Payoff linkage)
-- ═══════════════════════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS foreshadowing_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    -- Setup info
    setup_chapter INT NOT NULL,
    setup_description TEXT NOT NULL,   -- what was planted
    setup_context TEXT,                -- surrounding text excerpt
    -- Payoff info (NULL if unresolved)
    payoff_chapter INT,
    payoff_description TEXT,
    payoff_context TEXT,
    -- Metadata
    confidence REAL DEFAULT 0.5,       -- AI confidence in this detection
    status TEXT NOT NULL DEFAULT 'unresolved'
        CHECK (status IN ('unresolved', 'resolved', 'red_herring', 'dismissed')),
    category TEXT DEFAULT 'mystery'
        CHECK (category IN ('mystery', 'chekhov_gun', 'prophecy', 'character_secret', 'world_rule', 'other')),
    related_entity_ids UUID[] DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ
);

CREATE INDEX idx_foreshadowing_book_status ON foreshadowing_entries(book_id, status);

-- ═══════════════════════════════════════════════════════════════════════════════
-- Character State Changes (multi-dimensional state tracking)
-- ═══════════════════════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS character_state_changes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    entity_id UUID REFERENCES entities(id) ON DELETE SET NULL,
    character_name TEXT NOT NULL,
    chapter_index INT NOT NULL,
    -- State dimensions
    state_type TEXT NOT NULL
        CHECK (state_type IN ('health', 'mood', 'power', 'social_status', 'relationship', 'knowledge', 'possession')),
    from_state TEXT,
    to_state TEXT NOT NULL,
    -- Context
    trigger_event TEXT,                -- what caused the change
    context_snippet TEXT,
    significance REAL DEFAULT 0.5,     -- 0-1 importance
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_char_state_book ON character_state_changes(book_id, character_name);
CREATE INDEX idx_char_state_chapter ON character_state_changes(book_id, chapter_index);

-- ═══════════════════════════════════════════════════════════════════════════════
-- Scene Transitions (location tracking + character movement)
-- ═══════════════════════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS scene_transitions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    chapter_index INT NOT NULL,
    paragraph_index INT DEFAULT 0,
    -- Transition
    from_location TEXT,
    to_location TEXT NOT NULL,
    -- Who's involved
    characters_present TEXT[] DEFAULT '{}',
    -- Context
    transition_type TEXT DEFAULT 'move'
        CHECK (transition_type IN ('move', 'flashback', 'dream', 'parallel', 'timeskip')),
    context_snippet TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_scene_book_chapter ON scene_transitions(book_id, chapter_index);

-- ═══════════════════════════════════════════════════════════════════════════════
-- Macro Analysis Windows (periodic high-level arc analysis)
-- ═══════════════════════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS macro_analysis (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    start_chapter INT NOT NULL,
    end_chapter INT NOT NULL,
    -- Analysis results
    plot_arc TEXT,                      -- 起承转合 description
    key_conflicts TEXT[] DEFAULT '{}',
    active_relations JSONB,            -- relationship changes in this window
    resolved_mysteries TEXT[] DEFAULT '{}',
    new_mysteries TEXT[] DEFAULT '{}',
    arc_summary TEXT,
    raw_json JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(book_id, start_chapter, end_chapter)
);
