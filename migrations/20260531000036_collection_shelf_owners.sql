-- Owner anchors for empty personal organization containers.
ALTER TABLE collections ADD COLUMN IF NOT EXISTS owner_id UUID REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE shelves ADD COLUMN IF NOT EXISTS owner_id UUID REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE smart_shelves ADD COLUMN IF NOT EXISTS owner_id UUID REFERENCES users(id) ON DELETE SET NULL;

WITH singleton_owner AS (
    SELECT id
    FROM users
    WHERE (SELECT COUNT(*) FROM users) = 1
    ORDER BY created_at ASC, id ASC
    LIMIT 1
)
UPDATE collections
SET owner_id = singleton_owner.id
FROM singleton_owner
WHERE collections.owner_id IS NULL;

WITH singleton_owner AS (
    SELECT id
    FROM users
    WHERE (SELECT COUNT(*) FROM users) = 1
    ORDER BY created_at ASC, id ASC
    LIMIT 1
)
UPDATE shelves
SET owner_id = singleton_owner.id
FROM singleton_owner
WHERE shelves.owner_id IS NULL
  AND shelves.is_system = false;

WITH singleton_owner AS (
    SELECT id
    FROM users
    WHERE (SELECT COUNT(*) FROM users) = 1
    ORDER BY created_at ASC, id ASC
    LIMIT 1
)
UPDATE smart_shelves
SET owner_id = singleton_owner.id
FROM singleton_owner
WHERE smart_shelves.owner_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_collections_owner ON collections(owner_id);
CREATE INDEX IF NOT EXISTS idx_shelves_owner ON shelves(owner_id);
CREATE INDEX IF NOT EXISTS idx_smart_shelves_owner ON smart_shelves(owner_id);
