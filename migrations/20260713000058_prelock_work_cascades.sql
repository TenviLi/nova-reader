-- Deleting a work applies `ON DELETE SET NULL` to books.work_id without
-- firing the child books statement trigger. Work membership is part of the
-- duplicate-resolution lock closure, so protect the parent statement too.

DROP TRIGGER IF EXISTS trg_require_novel_dedup_work_delete_barrier ON book_works;
CREATE TRIGGER trg_require_novel_dedup_work_delete_barrier
BEFORE DELETE ON book_works
FOR EACH STATEMENT
EXECUTE FUNCTION require_novel_dedup_global_barrier();

-- Correct the earlier parent-cascade description: library deletion detaches
-- books with SET NULL; it does not delete their chapter rows.
COMMENT ON TRIGGER trg_require_novel_dedup_library_delete_barrier ON libraries IS
    'Acquires the dedup global write barrier before cascading library_id SET NULL into books.';
COMMENT ON TRIGGER trg_require_novel_dedup_work_delete_barrier ON book_works IS
    'Acquires the dedup global write barrier before cascading work_id SET NULL into books.';
