-- Allow annotations and bookmarks to be created before chapters are parsed
ALTER TABLE annotations ALTER COLUMN chapter_id DROP NOT NULL;
ALTER TABLE bookmarks ALTER COLUMN chapter_id DROP NOT NULL;
