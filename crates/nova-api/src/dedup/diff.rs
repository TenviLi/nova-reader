use serde::Serialize;

const MAX_CHANGES: usize = 500;
const MAX_CHANGE_CHARACTERS: usize = 1_000;

#[derive(Debug, Serialize)]
pub(crate) struct ChapterDiffChange {
    pub tag: &'static str,
    pub value: String,
}

pub(crate) struct ChapterTextDiff {
    pub character_count_a: usize,
    pub character_count_b: usize,
    pub changes: Vec<ChapterDiffChange>,
    pub ratio: f32,
    pub truncated: bool,
}

/// Compare two chapter bodies after the same conservative normalization used
/// by deterministic duplicate detection.
pub(crate) fn compare_chapter_texts(content_a: &str, content_b: &str) -> ChapterTextDiff {
    let text_a = nova_ingest::dedup::normalize_conservative(content_a);
    let text_b = nova_ingest::dedup::normalize_conservative(content_b);
    let diff = similar::TextDiff::from_words(&text_a, &text_b);
    let mut changes = Vec::new();
    let mut truncated = false;

    for change in diff.iter_all_changes() {
        if changes.len() >= MAX_CHANGES {
            truncated = true;
            break;
        }
        let tag = match change.tag() {
            similar::ChangeTag::Delete => "delete",
            similar::ChangeTag::Insert => "insert",
            similar::ChangeTag::Equal => "equal",
        };
        let value = change.value().chars().take(MAX_CHANGE_CHARACTERS).collect();
        changes.push(ChapterDiffChange { tag, value });
    }

    ChapterTextDiff {
        character_count_a: text_a.chars().count(),
        character_count_b: text_b.chars().count(),
        changes,
        ratio: diff.ratio(),
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_chapters_produce_only_equal_changes() {
        let diff = compare_chapter_texts("第一章\n相同 正文", "第一章\n相同 正文");

        assert_eq!(diff.ratio, 1.0);
        assert!(!diff.truncated);
        assert!(!diff.changes.is_empty());
        assert!(diff.changes.iter().all(|change| change.tag == "equal"));
    }
}
