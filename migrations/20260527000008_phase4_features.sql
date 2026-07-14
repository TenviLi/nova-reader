-- Phase 4 features: collection sharing, smart shelves, custom metadata fields, shelf ordering

-- Collection sharing (public links)
CREATE TABLE IF NOT EXISTS collection_shares (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    collection_id UUID NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    token VARCHAR(32) NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    CONSTRAINT uq_collection_shares_collection UNIQUE (collection_id)
);

CREATE INDEX idx_collection_shares_token ON collection_shares(token);

-- Smart shelves with dynamic filter criteria
CREATE TABLE IF NOT EXISTS smart_shelves (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    filter_criteria JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Add sort_order to shelf_books for drag-and-drop reordering
ALTER TABLE shelf_books ADD COLUMN IF NOT EXISTS sort_order INT NOT NULL DEFAULT 0;

-- Add custom_fields JSONB column to books for user-defined metadata schemas
ALTER TABLE books ADD COLUMN IF NOT EXISTS custom_fields JSONB DEFAULT '{}';

-- Index for custom fields searches
CREATE INDEX IF NOT EXISTS idx_books_custom_fields ON books USING GIN (custom_fields);

-- Annotation sharing (public links)
CREATE TABLE IF NOT EXISTS annotation_shares (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    annotation_id UUID NOT NULL REFERENCES annotations(id) ON DELETE CASCADE,
    token VARCHAR(32) NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_annotation_shares_annotation UNIQUE (annotation_id)
);

CREATE INDEX idx_annotation_shares_token ON annotation_shares(token);
