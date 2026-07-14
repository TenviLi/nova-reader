-- Nova Reader PostgreSQL Initialization
-- This script runs on first database creation

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";     -- Trigram similarity for fuzzy search
CREATE EXTENSION IF NOT EXISTS "vector";       -- pgvector for embeddings
CREATE EXTENSION IF NOT EXISTS "unaccent";     -- Accent-insensitive search

-- Create custom text search configuration for CJK support
-- (Chinese/Japanese/Korean novels)
CREATE TEXT SEARCH CONFIGURATION IF NOT EXISTS nova_fts (COPY = simple);
