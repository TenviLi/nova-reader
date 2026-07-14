-- Make the canonical acquisition order explicit. Migration 50 introduced the
-- shared lock seam; this loop guarantees PostgreSQL invokes the volatile lock
-- function one UUID at a time in sorted order.

CREATE OR REPLACE FUNCTION lock_novel_dedup_books(book_ids UUID[])
RETURNS VOID
LANGUAGE plpgsql
AS $$
DECLARE
    locked_book_id UUID;
BEGIN
    FOR locked_book_id IN
        SELECT DISTINCT book_id
        FROM unnest(book_ids) AS book_id
        WHERE book_id IS NOT NULL
        ORDER BY book_id
    LOOP
        PERFORM pg_advisory_xact_lock(
            hashtextextended('nova:dedup:book:' || locked_book_id::text, 0)
        );
    END LOOP;
END;
$$;
