-- Collaborative annotations (book club mode)
-- Users can annotate passages, reply to each other, and react with emojis

CREATE TABLE IF NOT EXISTS annotations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    book_id UUID NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    chapter_index INTEGER NOT NULL,
    start_offset INTEGER NOT NULL,
    end_offset INTEGER NOT NULL,
    highlighted_text TEXT NOT NULL,
    note TEXT NOT NULL,
    visibility VARCHAR(20) NOT NULL DEFAULT 'club', -- 'private', 'club', 'public'
    club_id UUID REFERENCES book_clubs(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS book_clubs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    book_id UUID REFERENCES books(id) ON DELETE SET NULL,
    invite_code VARCHAR(12) UNIQUE NOT NULL,
    max_members INTEGER NOT NULL DEFAULT 20,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS book_club_members (
    club_id UUID NOT NULL REFERENCES book_clubs(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(20) NOT NULL DEFAULT 'member', -- 'owner', 'moderator', 'member'
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (club_id, user_id)
);

CREATE TABLE IF NOT EXISTS annotation_replies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    annotation_id UUID NOT NULL REFERENCES annotations(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS annotation_reactions (
    annotation_id UUID NOT NULL REFERENCES annotations(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    emoji VARCHAR(10) NOT NULL DEFAULT '❤️',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (annotation_id, user_id)
);

ALTER TABLE annotations ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id) ON DELETE CASCADE;
ALTER TABLE annotations ADD COLUMN IF NOT EXISTS chapter_index INTEGER;
ALTER TABLE annotations ADD COLUMN IF NOT EXISTS highlighted_text TEXT;
ALTER TABLE annotations ADD COLUMN IF NOT EXISTS visibility VARCHAR(20) NOT NULL DEFAULT 'club';
ALTER TABLE annotations ADD COLUMN IF NOT EXISTS club_id UUID REFERENCES book_clubs(id) ON DELETE SET NULL;

-- Indexes for query performance
CREATE INDEX IF NOT EXISTS idx_annotations_book_chapter ON annotations(book_id, chapter_index);
CREATE INDEX IF NOT EXISTS idx_annotations_user ON annotations(user_id);
CREATE INDEX IF NOT EXISTS idx_annotations_club ON annotations(club_id) WHERE club_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_book_club_members_user ON book_club_members(user_id);
CREATE INDEX IF NOT EXISTS idx_annotation_replies_annotation ON annotation_replies(annotation_id);
