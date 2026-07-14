-- Algorithm v5 narrows exact_content from layout/NFKC equality to the
-- conservative normalization contract.  Old v4 pairs must not remain visible
-- as current evidence between deployment and the next scan.
UPDATE duplicate_pairs
SET stale = TRUE,
    updated_at = NOW()
WHERE stale = FALSE
  AND algorithm_version < 5;

COMMENT ON COLUMN duplicate_pairs.algorithm_version IS
    'Dedup classifier version; rows from older versions are marked stale by the corresponding upgrade migration.';
