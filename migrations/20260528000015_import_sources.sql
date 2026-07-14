-- Import sources: web scraping + RSS/Atom monitoring for chapter updates

CREATE TYPE import_source_type AS ENUM ('web_scraper', 'rss', 'atom', 'api');
CREATE TYPE import_status AS ENUM ('active', 'paused', 'error', 'completed');

CREATE TABLE IF NOT EXISTS import_sources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    book_id UUID REFERENCES books(id) ON DELETE SET NULL, -- linked book (null if not yet created)
    name VARCHAR(255) NOT NULL,
    source_type import_source_type NOT NULL,
    url TEXT NOT NULL, -- base URL or RSS feed URL
    config JSONB NOT NULL DEFAULT '{}', -- scraper rules, selectors, etc.
    status import_status NOT NULL DEFAULT 'active',
    last_check_at TIMESTAMPTZ,
    last_chapter_at TIMESTAMPTZ,
    check_interval_minutes INTEGER NOT NULL DEFAULT 60,
    total_chapters_imported INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Config JSONB schema for web_scraper:
-- {
--   "chapter_list_selector": "CSS selector for chapter links",
--   "chapter_content_selector": "CSS selector for chapter body",
--   "title_selector": "CSS selector for chapter title",
--   "next_page_selector": "CSS selector for pagination",
--   "encoding": "utf-8|gbk|gb2312",
--   "delay_ms": 2000,
--   "headers": {"User-Agent": "..."},
--   "cookie": "..."
-- }

CREATE TABLE IF NOT EXISTS import_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_id UUID NOT NULL REFERENCES import_sources(id) ON DELETE CASCADE,
    action VARCHAR(50) NOT NULL, -- 'check', 'import_chapter', 'error', 'complete'
    chapter_title VARCHAR(500),
    chapter_index INTEGER,
    details JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_import_sources_user ON import_sources(user_id);
CREATE INDEX IF NOT EXISTS idx_import_sources_status ON import_sources(status) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_import_sources_next_check ON import_sources(last_check_at, check_interval_minutes)
    WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_import_logs_source ON import_logs(source_id, created_at DESC);
