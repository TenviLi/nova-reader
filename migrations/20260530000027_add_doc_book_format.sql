-- Add legacy Word .doc as a first-class book format.
ALTER TYPE book_format ADD VALUE IF NOT EXISTS 'doc';