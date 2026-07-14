-- Sprint 9: Trope Ontology & Persona Drift
-- Self-evolving hierarchical setting taxonomy + character voice drift detection

-- ═══════════════════════════════════════════════════════════════════════════════
-- 1. Trope Ontology Tree: Auto-growing hierarchical taxonomy of settings/tropes
-- ═══════════════════════════════════════════════════════════════════════════════

-- Each node in the ontology tree is a cluster discovered via HDBSCAN on chunk embeddings
CREATE TABLE IF NOT EXISTS trope_nodes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    parent_id UUID REFERENCES trope_nodes(id) ON DELETE SET NULL,
    -- Machine-generated fields
    label TEXT NOT NULL,                      -- LLM-summarized name, e.g. "科技系皮物 - 纳米机器人型"
    description TEXT,                         -- LLM-generated description of this cluster
    level INT NOT NULL DEFAULT 0,             -- Depth in tree (0 = root categories)
    -- Cluster stats
    centroid BYTEA,                           -- Serialized f32 centroid vector (2560-dim = 10240 bytes)
    cluster_size INT NOT NULL DEFAULT 0,      -- Number of chunks in this cluster
    stability FLOAT NOT NULL DEFAULT 0.0,     -- HDBSCAN stability score
    -- Semantic attributes extracted by LLM
    attributes JSONB NOT NULL DEFAULT '{}',   -- Structured schema: {"memory_retained": true, "pain_feedback": 0.8, ...}
    -- Metadata
    domain TEXT NOT NULL DEFAULT 'general',   -- Domain partition: 'tsf', 'possession', 'fusion', 'parasite', 'general'
    is_leaf BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_trope_nodes_parent ON trope_nodes(parent_id);
CREATE INDEX idx_trope_nodes_domain ON trope_nodes(domain);
CREATE INDEX idx_trope_nodes_level ON trope_nodes(level);

-- Links chunks (identified by Qdrant point ID) to their trope node
CREATE TABLE IF NOT EXISTS trope_chunk_assignments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    trope_node_id UUID NOT NULL REFERENCES trope_nodes(id) ON DELETE CASCADE,
    book_id UUID NOT NULL,
    chapter_index INT NOT NULL,
    chunk_index INT NOT NULL,
    qdrant_point_id BIGINT NOT NULL,          -- Deterministic hash used in Qdrant
    membership_score FLOAT NOT NULL,          -- HDBSCAN membership probability [0,1]
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_trope_chunks_node ON trope_chunk_assignments(trope_node_id);
CREATE INDEX idx_trope_chunks_book ON trope_chunk_assignments(book_id);
CREATE UNIQUE INDEX idx_trope_chunks_point ON trope_chunk_assignments(qdrant_point_id);

-- Extracted fine-grained attributes per book (LLM-structured output)
-- e.g. "Is memory retained?", "Pain feedback level", "Detachment side effects"
CREATE TABLE IF NOT EXISTS trope_book_attributes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    book_id UUID NOT NULL,
    trope_node_id UUID NOT NULL REFERENCES trope_nodes(id) ON DELETE CASCADE,
    -- Structured attribute extraction
    attribute_key TEXT NOT NULL,              -- e.g. "memory_retained", "pain_feedback", "side_effects"
    attribute_value JSONB NOT NULL,           -- Can be bool, number, string, or array
    confidence FLOAT NOT NULL DEFAULT 0.5,   -- LLM confidence [0,1]
    evidence_chunks JSONB DEFAULT '[]',      -- Array of chunk references that support this
    extracted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_trope_attrs_book ON trope_book_attributes(book_id);
CREATE INDEX idx_trope_attrs_node ON trope_book_attributes(trope_node_id);
CREATE INDEX idx_trope_attrs_key ON trope_book_attributes(attribute_key);
CREATE UNIQUE INDEX idx_trope_attrs_unique ON trope_book_attributes(book_id, trope_node_id, attribute_key);

-- ═══════════════════════════════════════════════════════════════════════════════
-- 2. Persona Drift: Track character voice/personality changes across chapters
-- ═══════════════════════════════════════════════════════════════════════════════

-- Per-chapter persona embedding for a tracked character
CREATE TABLE IF NOT EXISTS persona_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    book_id UUID NOT NULL,
    entity_id UUID,                           -- Reference to entity in PG (if extracted)
    character_name TEXT NOT NULL,             -- Character name as fallback
    chapter_index INT NOT NULL,
    -- Embeddings of this character's voice at this chapter
    dialogue_centroid BYTEA,                 -- Centroid of all dialogue chunks (2560-dim)
    monologue_centroid BYTEA,                -- Centroid of internal monologue chunks
    dialogue_count INT NOT NULL DEFAULT 0,   -- Number of dialogue segments
    monologue_count INT NOT NULL DEFAULT 0,
    -- Drift metrics (compared to previous chapter)
    drift_from_prev FLOAT,                   -- Cosine distance from previous chapter
    drift_from_baseline FLOAT,               -- Cosine distance from chapter 1 (original persona)
    computed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_persona_book_char ON persona_snapshots(book_id, character_name);
CREATE UNIQUE INDEX idx_persona_unique ON persona_snapshots(book_id, character_name, chapter_index);

-- Detected drift events (high-magnitude changes worth highlighting)
CREATE TABLE IF NOT EXISTS persona_drift_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    book_id UUID NOT NULL,
    character_name TEXT NOT NULL,
    chapter_index INT NOT NULL,
    drift_magnitude FLOAT NOT NULL,          -- How big the shift was
    drift_direction TEXT,                     -- LLM-described: "向被附身者靠拢", "情绪失控", etc.
    -- What changed
    evidence_text TEXT,                       -- Representative passage showing the drift
    target_persona TEXT,                      -- If drifting toward another character's voice
    event_type TEXT NOT NULL DEFAULT 'drift', -- 'drift', 'fusion', 'reversion', 'awakening'
    detected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_drift_events_book ON persona_drift_events(book_id, character_name);
CREATE INDEX idx_drift_events_magnitude ON persona_drift_events(drift_magnitude DESC);

-- ═══════════════════════════════════════════════════════════════════════════════
-- 3. Cross-Book Rule Splicing: Store extracted "rules" as graph-ready triples
-- ═══════════════════════════════════════════════════════════════════════════════

-- A rule is a structured setting fact extracted from text
-- e.g. "穿戴者 → [控制, {痛觉反馈: 50%, 精神消耗: 中}] → 皮物躯体"
CREATE TABLE IF NOT EXISTS setting_rules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    book_id UUID NOT NULL,
    trope_node_id UUID REFERENCES trope_nodes(id) ON DELETE SET NULL,
    -- Triple structure
    subject_type TEXT NOT NULL,               -- 'character', 'vessel', 'soul', 'item', 'mechanism'
    subject_label TEXT NOT NULL,              -- "穿戴者", "附身灵", "宿主"
    predicate TEXT NOT NULL,                  -- "controls", "inhabits", "fuses_with", "parasitizes"
    object_type TEXT NOT NULL,
    object_label TEXT NOT NULL,
    -- Structured properties of this rule
    properties JSONB NOT NULL DEFAULT '{}',   -- {"pain_feedback": 0.5, "duration": "permanent", ...}
    constraints JSONB DEFAULT '[]',           -- ["需要精神力维持", "脱下后24小时内可再次穿戴"]
    -- Provenance
    source_text TEXT,                         -- Original passage this was extracted from
    chapter_index INT,
    confidence FLOAT NOT NULL DEFAULT 0.5,
    extracted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_rules_book ON setting_rules(book_id);
CREATE INDEX idx_rules_trope ON setting_rules(trope_node_id);
CREATE INDEX idx_rules_predicate ON setting_rules(predicate);

-- ═══════════════════════════════════════════════════════════════════════════════
-- 4. Ontology Evolution Log: Track how the tree grows over time
-- ═══════════════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS ontology_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type TEXT NOT NULL,                 -- 'node_created', 'node_merged', 'node_split', 'node_relabeled'
    trope_node_id UUID REFERENCES trope_nodes(id) ON DELETE SET NULL,
    details JSONB NOT NULL DEFAULT '{}',      -- Event-specific payload
    triggered_by TEXT,                        -- 'clustering', 'manual', 'new_book_ingest'
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_ontology_events_type ON ontology_events(event_type);
CREATE INDEX idx_ontology_events_time ON ontology_events(created_at DESC);
