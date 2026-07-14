-- Durable incremental deduplication requests and exact-file import evidence.
--
-- Chapter writes can originate from HTTP handlers, parsers, maintenance jobs,
-- or future filesystem workers. Keep the automatic rescan boundary in
-- PostgreSQL so every committed content change is covered. Requests for one
-- library coalesce into at most one queued follow-up task and are delayed by
-- 500ms to debounce multi-chapter imports.

CREATE TABLE IF NOT EXISTS exact_file_discoveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    library_id UUID REFERENCES libraries(id) ON DELETE SET NULL,
    matched_book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    source_kind VARCHAR(24) NOT NULL,
    source_key VARCHAR(64) NOT NULL,
    source_path TEXT NOT NULL,
    file_hash VARCHAR(128) NOT NULL,
    file_size_bytes BIGINT NOT NULL DEFAULT 0,
    discovered_by UUID REFERENCES users(id) ON DELETE SET NULL,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    seen_count BIGINT NOT NULL DEFAULT 1,
    CONSTRAINT chk_exact_file_discoveries_source_kind
        CHECK (source_kind IN ('upload', 'library_scan')),
    CONSTRAINT chk_exact_file_discoveries_file_size
        CHECK (file_size_bytes >= 0),
    CONSTRAINT chk_exact_file_discoveries_seen_count
        CHECK (seen_count > 0),
    CONSTRAINT uq_exact_file_discovery_identity
        UNIQUE (matched_book_id, source_kind, source_key, file_hash)
);

CREATE INDEX IF NOT EXISTS idx_exact_file_discoveries_library_seen
    ON exact_file_discoveries(library_id, last_seen_at DESC);
CREATE INDEX IF NOT EXISTS idx_exact_file_discoveries_matched_book
    ON exact_file_discoveries(matched_book_id, last_seen_at DESC);

COMMENT ON TABLE exact_file_discoveries IS
    'Files skipped during import because their raw SHA-256 matches an existing book';
COMMENT ON COLUMN exact_file_discoveries.source_key IS
    'SHA-256 of the source kind and source path, used for bounded idempotency keys';

CREATE OR REPLACE FUNCTION enqueue_targeted_novel_dedup_scan(changed_book_id UUID)
RETURNS UUID
LANGUAGE plpgsql
AS $$
DECLARE
    scan_library_id UUID;
    queued_scan_id UUID;
    queued_task_id UUID;
    queued_payload JSONB;
    merged_payload JSONB;
    merged_targets UUID[];
    scan_task_id UUID;
    scan_id UUID;
    scan_books_total INTEGER;
BEGIN
    SELECT library_id
    INTO scan_library_id
    FROM books
    WHERE id = changed_book_id
      AND status NOT IN ('archived', 'duplicate');

    IF NOT FOUND THEN
        RETURN NULL;
    END IF;

    -- This is the same per-scope transaction lock used by the Rust enqueue
    -- path. It serializes trigger-driven and API-driven coalescing.
    PERFORM pg_advisory_xact_lock(
        hashtextextended(COALESCE(scan_library_id::text, '__all_libraries__'), 0)
    );

    SELECT scan.id, task.id, task.payload
    INTO queued_scan_id, queued_task_id, queued_payload
    FROM dedup_scan_runs scan
    JOIN tasks task ON task.id = scan.task_id
    WHERE scan.library_id IS NOT DISTINCT FROM scan_library_id
      AND scan.status = 'queued'
      AND task.status = 'queued'::task_status
    ORDER BY scan.created_at DESC
    LIMIT 1
    FOR UPDATE OF scan, task;

    IF FOUND THEN
        -- A missing/null target list denotes a full scan and already covers
        -- this book. Otherwise merge and sort the targeted UUID set.
        IF jsonb_typeof(queued_payload->'target_book_ids') = 'array' THEN
            SELECT ARRAY(
                SELECT target_id
                FROM (
                    SELECT value::UUID AS target_id
                    FROM jsonb_array_elements_text(queued_payload->'target_book_ids')
                    UNION
                    SELECT changed_book_id
                ) targets
                ORDER BY target_id
            )
            INTO merged_targets;

            merged_payload := jsonb_set(
                queued_payload,
                '{target_book_ids}',
                to_jsonb(merged_targets),
                TRUE
            );
        ELSE
            merged_payload := queued_payload;
            merged_targets := NULL;
        END IF;

        SELECT COUNT(*)::INTEGER
        INTO scan_books_total
        FROM books
        WHERE status NOT IN ('archived', 'duplicate')
          AND (scan_library_id IS NULL OR library_id = scan_library_id)
          AND (merged_targets IS NULL OR id = ANY(merged_targets));

        UPDATE tasks
        SET payload = merged_payload,
            scheduled_at = GREATEST(
                scheduled_at,
                statement_timestamp() + INTERVAL '500 milliseconds'
            )
        WHERE id = queued_task_id;

        UPDATE dedup_scan_runs
        SET books_total = scan_books_total,
            updated_at = NOW()
        WHERE id = queued_scan_id;

        RETURN queued_scan_id;
    END IF;

    scan_id := gen_random_uuid();
    scan_task_id := gen_random_uuid();
    merged_targets := ARRAY[changed_book_id];
    merged_payload := jsonb_build_object(
        'operation', 'scan',
        'scan_run_id', scan_id,
        'library_id', scan_library_id,
        'include_semantic', FALSE,
        'target_book_ids', to_jsonb(merged_targets)
    );

    SELECT COUNT(*)::INTEGER
    INTO scan_books_total
    FROM books
    WHERE status NOT IN ('archived', 'duplicate')
      AND (scan_library_id IS NULL OR library_id = scan_library_id)
      AND id = changed_book_id;

    INSERT INTO tasks (
        id, kind, status, priority, payload, book_id, category,
        max_retries, scheduled_at
    )
    VALUES (
        scan_task_id, 'deduplicate'::task_kind, 'queued'::task_status,
        '1'::task_priority, merged_payload, changed_book_id, 'preprocess',
        3, statement_timestamp() + INTERVAL '500 milliseconds'
    );

    INSERT INTO task_execution_locks (task_id, resource_key, mode)
    VALUES (
        scan_task_id,
        'dedup:scan:barrier',
        CASE WHEN scan_library_id IS NULL THEN 'exclusive' ELSE 'shared' END
    );

    IF scan_library_id IS NOT NULL THEN
        INSERT INTO task_execution_locks (task_id, resource_key, mode)
        VALUES (
            scan_task_id,
            'dedup:scan:library:' || scan_library_id::text,
            'exclusive'
        );
    END IF;

    INSERT INTO dedup_scan_runs (
        id, library_id, requested_by, task_id, include_semantic,
        algorithm_version, books_total
    )
    VALUES (
        scan_id, scan_library_id, NULL, scan_task_id, FALSE, 3,
        scan_books_total
    );

    RETURN scan_id;
END;
$$;

CREATE OR REPLACE FUNCTION schedule_novel_dedup_after_chapter_change()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'UPDATE'
       AND OLD.book_id IS NOT DISTINCT FROM NEW.book_id
       AND OLD.chapter_index IS NOT DISTINCT FROM NEW.chapter_index
       AND OLD.content IS NOT DISTINCT FROM NEW.content THEN
        RETURN NEW;
    END IF;

    IF TG_OP = 'DELETE' THEN
        PERFORM enqueue_targeted_novel_dedup_scan(OLD.book_id);
        RETURN OLD;
    END IF;

    PERFORM enqueue_targeted_novel_dedup_scan(NEW.book_id);

    IF TG_OP = 'UPDATE' AND OLD.book_id IS DISTINCT FROM NEW.book_id THEN
        PERFORM enqueue_targeted_novel_dedup_scan(OLD.book_id);
    END IF;

    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION schedule_novel_dedup_after_book_import()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    PERFORM enqueue_targeted_novel_dedup_scan(NEW.id);
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_schedule_novel_dedup_after_book_import ON books;
CREATE TRIGGER trg_schedule_novel_dedup_after_book_import
AFTER INSERT ON books
FOR EACH ROW
EXECUTE FUNCTION schedule_novel_dedup_after_book_import();

DROP TRIGGER IF EXISTS trg_schedule_novel_dedup_after_chapter_change ON chapters;
CREATE TRIGGER trg_schedule_novel_dedup_after_chapter_change
AFTER INSERT OR DELETE OR UPDATE OF book_id, chapter_index, content ON chapters
FOR EACH ROW
EXECUTE FUNCTION schedule_novel_dedup_after_chapter_change();
