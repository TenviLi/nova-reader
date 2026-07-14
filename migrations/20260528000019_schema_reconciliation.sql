-- Schema reconciliation: add missing columns referenced by Rust structs
-- This resolves mismatches between code queries and actual DB schema

-- 1. reading_progress: add chapter_index, updated_at
ALTER TABLE reading_progress ADD COLUMN IF NOT EXISTS chapter_index INTEGER;
ALTER TABLE reading_progress ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- 2. annotations: add chapter_index (integer reference for convenience alongside chapter_id)
ALTER TABLE annotations ADD COLUMN IF NOT EXISTS chapter_index INTEGER;

-- 3. bookmarks: add chapter_index and position columns
ALTER TABLE bookmarks ADD COLUMN IF NOT EXISTS chapter_index INTEGER;
ALTER TABLE bookmarks ADD COLUMN IF NOT EXISTS position DOUBLE PRECISION;
ALTER TABLE bookmarks ADD COLUMN IF NOT EXISTS title TEXT;

-- 4. reading_sessions: add start_chapter, end_chapter, end_time, updated_at, words_read
ALTER TABLE reading_sessions ADD COLUMN IF NOT EXISTS start_chapter INTEGER;
ALTER TABLE reading_sessions ADD COLUMN IF NOT EXISTS end_chapter INTEGER;
ALTER TABLE reading_sessions ADD COLUMN IF NOT EXISTS end_time TIMESTAMPTZ;
ALTER TABLE reading_sessions ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ DEFAULT NOW();
ALTER TABLE reading_sessions ADD COLUMN IF NOT EXISTS words_read BIGINT DEFAULT 0;

-- 5. shelves: add is_ordered, book_count (denormalized)
ALTER TABLE shelves ADD COLUMN IF NOT EXISTS is_ordered BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE shelves ADD COLUMN IF NOT EXISTS book_count INTEGER NOT NULL DEFAULT 0;

-- 6. collections: add book_count (denormalized)
ALTER TABLE collections ADD COLUMN IF NOT EXISTS book_count INTEGER NOT NULL DEFAULT 0;

-- 7. entity_mentions: add book_id, chapter_index, position_start, position_end
ALTER TABLE entity_mentions ADD COLUMN IF NOT EXISTS book_id UUID REFERENCES books(id) ON DELETE CASCADE;
ALTER TABLE entity_mentions ADD COLUMN IF NOT EXISTS chapter_index INTEGER;
ALTER TABLE entity_mentions ADD COLUMN IF NOT EXISTS position_start BIGINT;
ALTER TABLE entity_mentions ADD COLUMN IF NOT EXISTS position_end BIGINT;

-- Backfill entity_mentions.book_id from chapters
UPDATE entity_mentions em
SET book_id = c.book_id,
    chapter_index = c.index,
    position_start = em.start_offset,
    position_end = em.end_offset
FROM chapters c
WHERE em.chapter_id = c.id AND em.book_id IS NULL;

-- 8. entity_relationships: add relationship_type alias column
ALTER TABLE entity_relationships ADD COLUMN IF NOT EXISTS relationship_type TEXT;
UPDATE entity_relationships SET relationship_type = relation_type WHERE relationship_type IS NULL;

-- 9. entity_profiles: add id, summary, traits, first_appearance, last_appearance
ALTER TABLE entity_profiles ADD COLUMN IF NOT EXISTS id UUID DEFAULT gen_random_uuid();
ALTER TABLE entity_profiles ADD COLUMN IF NOT EXISTS summary TEXT;
ALTER TABLE entity_profiles ADD COLUMN IF NOT EXISTS traits JSONB DEFAULT '[]'::jsonb;
ALTER TABLE entity_profiles ADD COLUMN IF NOT EXISTS first_appearance TEXT;
ALTER TABLE entity_profiles ADD COLUMN IF NOT EXISTS last_appearance TEXT;
-- Backfill summary from arc_summary
UPDATE entity_profiles SET summary = arc_summary WHERE summary IS NULL AND arc_summary IS NOT NULL;

-- 10. books: add tags column (used by batch_process_book)
ALTER TABLE books ADD COLUMN IF NOT EXISTS tags JSONB DEFAULT '[]'::jsonb;

-- 11. entities: add unique constraint for upsert
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'entities_book_id_name_key') THEN
        ALTER TABLE entities ADD CONSTRAINT entities_book_id_name_key UNIQUE (book_id, name);
    END IF;
EXCEPTION WHEN others THEN NULL;
END $$;

-- Update denormalized book_count on collections
UPDATE collections c SET book_count = (
    SELECT COUNT(*) FROM collection_books cb WHERE cb.collection_id = c.id
);

-- Update denormalized book_count on shelves
UPDATE shelves s SET book_count = (
    SELECT COUNT(*) FROM shelf_books sb WHERE sb.shelf_id = s.id
);
