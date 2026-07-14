-- PostgreSQL does not fire child-table statement triggers for rows reached by
-- an FK cascade. A top-level library DELETE can cascade through books and
-- chapters, so enter the global dedup barrier on the parent statement too.

DROP TRIGGER IF EXISTS trg_require_novel_dedup_library_delete_barrier ON libraries;
CREATE TRIGGER trg_require_novel_dedup_library_delete_barrier
BEFORE DELETE ON libraries
FOR EACH STATEMENT
EXECUTE FUNCTION require_novel_dedup_global_barrier();

COMMENT ON TRIGGER trg_require_novel_dedup_library_delete_barrier ON libraries IS
    'Acquires the dedup global write barrier before cascading through books and chapters.';
