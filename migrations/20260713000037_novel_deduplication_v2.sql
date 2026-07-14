-- Content-aware novel deduplication, version containment, and review workflow.

CREATE TABLE IF NOT EXISTS book_fingerprints (
    book_id UUID PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
    normalization_version INTEGER NOT NULL,
    algorithm_version INTEGER NOT NULL,
    source_content_hash VARCHAR(64) NOT NULL,
    conservative_hash VARCHAR(64) NOT NULL,
    layout_hash VARCHAR(64) NOT NULL,
    chapter_count INTEGER NOT NULL DEFAULT 0,
    informative_chapter_count INTEGER NOT NULL DEFAULT 0,
    char_count BIGINT NOT NULL DEFAULT 0,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_book_fingerprints_conservative
    ON book_fingerprints(conservative_hash);
CREATE INDEX IF NOT EXISTS idx_book_fingerprints_layout
    ON book_fingerprints(layout_hash);

CREATE TABLE IF NOT EXISTS chapter_fingerprints (
    chapter_id UUID PRIMARY KEY REFERENCES chapters(id) ON DELETE CASCADE,
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    chapter_index INTEGER NOT NULL,
    normalization_version INTEGER NOT NULL,
    source_content_hash VARCHAR(64) NOT NULL,
    conservative_hash VARCHAR(64) NOT NULL,
    layout_hash VARCHAR(64) NOT NULL,
    char_count BIGINT NOT NULL DEFAULT 0,
    informative BOOLEAN NOT NULL DEFAULT TRUE,
    winnowing_count INTEGER NOT NULL DEFAULT 0,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_chapter_fingerprints_book
    ON chapter_fingerprints(book_id, chapter_index);
CREATE INDEX IF NOT EXISTS idx_chapter_fingerprints_conservative
    ON chapter_fingerprints(conservative_hash, book_id)
    WHERE informative = TRUE;
CREATE INDEX IF NOT EXISTS idx_chapter_fingerprints_layout
    ON chapter_fingerprints(layout_hash, book_id)
    WHERE informative = TRUE;

CREATE TABLE IF NOT EXISTS passage_fingerprints (
    chapter_id UUID NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    normalization_version INTEGER NOT NULL,
    fingerprint_hash BIGINT NOT NULL,
    position INTEGER NOT NULL,
    span_length INTEGER NOT NULL,
    PRIMARY KEY (chapter_id, normalization_version, fingerprint_hash, position)
);

CREATE INDEX IF NOT EXISTS idx_passage_fingerprints_hash
    ON passage_fingerprints(fingerprint_hash, book_id);
CREATE INDEX IF NOT EXISTS idx_passage_fingerprints_book
    ON passage_fingerprints(book_id);

CREATE TABLE IF NOT EXISTS dedup_scan_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    library_id UUID REFERENCES libraries(id) ON DELETE CASCADE,
    requested_by UUID REFERENCES users(id) ON DELETE SET NULL,
    task_id UUID REFERENCES tasks(id) ON DELETE SET NULL,
    include_semantic BOOLEAN NOT NULL DEFAULT FALSE,
    algorithm_version INTEGER NOT NULL,
    status VARCHAR(24) NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'running', 'completed', 'failed', 'cancelled')),
    progress SMALLINT NOT NULL DEFAULT 0 CHECK (progress BETWEEN 0 AND 100),
    progress_message TEXT,
    books_total INTEGER NOT NULL DEFAULT 0,
    books_processed INTEGER NOT NULL DEFAULT 0,
    chapters_processed INTEGER NOT NULL DEFAULT 0,
    candidates_found INTEGER NOT NULL DEFAULT 0,
    pairs_found INTEGER NOT NULL DEFAULT 0,
    exact_pairs INTEGER NOT NULL DEFAULT 0,
    contained_pairs INTEGER NOT NULL DEFAULT 0,
    semantic_pairs INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_dedup_scan_runs_scope
    ON dedup_scan_runs(library_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_dedup_scan_runs_status
    ON dedup_scan_runs(status, created_at DESC);

-- Normalize any legacy directional pairs before enforcing a canonical order.
DELETE FROM duplicate_pairs older
USING duplicate_pairs newer
WHERE older.id < newer.id
  AND LEAST(older.book_a_id, older.book_b_id) = LEAST(newer.book_a_id, newer.book_b_id)
  AND GREATEST(older.book_a_id, older.book_b_id) = GREATEST(newer.book_a_id, newer.book_b_id);

UPDATE duplicate_pairs
SET book_a_id = LEAST(book_a_id, book_b_id),
    book_b_id = GREATEST(book_a_id, book_b_id)
WHERE book_a_id > book_b_id;

ALTER TABLE duplicate_pairs
    ADD COLUMN IF NOT EXISTS scan_run_id UUID REFERENCES dedup_scan_runs(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS relation VARCHAR(32) NOT NULL DEFAULT 'partial_overlap',
    ADD COLUMN IF NOT EXISTS review_status VARCHAR(24) NOT NULL DEFAULT 'pending',
    ADD COLUMN IF NOT EXISTS confidence DOUBLE PRECISION NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS shared_chapters INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS coverage_a DOUBLE PRECISION NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS coverage_b DOUBLE PRECISION NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS character_coverage_a DOUBLE PRECISION NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS character_coverage_b DOUBLE PRECISION NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS longest_contiguous_run INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS order_score DOUBLE PRECISION NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS contained_book_id UUID REFERENCES books(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS recommended_primary_id UUID REFERENCES books(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS semantic_score DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS algorithm_version INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS evidence JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS stale BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS resolved_by UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS resolved_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

ALTER TABLE duplicate_pairs DROP CONSTRAINT IF EXISTS duplicate_pairs_book_a_id_book_b_id_key;

CREATE UNIQUE INDEX IF NOT EXISTS idx_duplicate_pairs_canonical
    ON duplicate_pairs(book_a_id, book_b_id);
CREATE INDEX IF NOT EXISTS idx_duplicate_pairs_review
    ON duplicate_pairs(review_status, relation, confidence DESC)
    WHERE stale = FALSE;
CREATE INDEX IF NOT EXISTS idx_duplicate_pairs_scan
    ON duplicate_pairs(scan_run_id);

DO $$ BEGIN
    ALTER TABLE duplicate_pairs
        ADD CONSTRAINT chk_duplicate_pairs_canonical_order
        CHECK (book_a_id < book_b_id);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE duplicate_pairs
        ADD CONSTRAINT chk_duplicate_pairs_relation
        CHECK (relation IN (
            'exact_file', 'exact_content', 'contained_version',
            'high_overlap', 'partial_overlap', 'semantic_relation'
        ));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE duplicate_pairs
        ADD CONSTRAINT chk_duplicate_pairs_review_status
        CHECK (review_status IN ('pending', 'confirmed', 'dismissed', 'deferred'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE TABLE IF NOT EXISTS duplicate_chapter_matches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pair_id UUID NOT NULL REFERENCES duplicate_pairs(id) ON DELETE CASCADE,
    chapter_a_id UUID REFERENCES chapters(id) ON DELETE CASCADE,
    chapter_b_id UUID REFERENCES chapters(id) ON DELETE CASCADE,
    chapter_a_index INTEGER,
    chapter_b_index INTEGER,
    match_type VARCHAR(24) NOT NULL
        CHECK (match_type IN ('conservative', 'layout', 'winnowing', 'semantic')),
    similarity DOUBLE PRECISION NOT NULL DEFAULT 1,
    shared_fingerprints INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(pair_id, chapter_a_id, chapter_b_id)
);

CREATE INDEX IF NOT EXISTS idx_duplicate_chapter_matches_pair
    ON duplicate_chapter_matches(pair_id, chapter_a_index, chapter_b_index);

-- A work groups concrete book/file versions without discarding provenance.
CREATE TABLE IF NOT EXISTS book_works (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    canonical_title VARCHAR(1024) NOT NULL,
    canonical_author VARCHAR(512),
    primary_book_id UUID,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE books
    ADD COLUMN IF NOT EXISTS work_id UUID REFERENCES book_works(id) ON DELETE SET NULL;

DO $$ BEGIN
    ALTER TABLE book_works
        ADD CONSTRAINT book_works_primary_book_fk
        FOREIGN KEY (primary_book_id) REFERENCES books(id) ON DELETE SET NULL;
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE INDEX IF NOT EXISTS idx_books_work ON books(work_id);
CREATE INDEX IF NOT EXISTS idx_book_works_primary ON book_works(primary_book_id);
