use regex::Regex;
use std::sync::LazyLock;

use nova_core::domain::chapter::ChunkingConfig;

/// Splits raw text into chapters using pattern matching and heuristics.
pub struct ChapterSplitter {
    config: ChunkingConfig,
}

/// A detected chapter boundary.
#[derive(Debug, Clone)]
pub struct DetectedChapter {
    pub index: i32,
    pub title: Option<String>,
    pub content: String,
    pub start_offset: usize,
    pub end_offset: usize,
    pub word_count: i32,
}

// Chinese chapter patterns (most common in web novels)
static CN_CHAPTER_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?m)^[\s　]*第[一二三四五六七八九十百千万零〇\d]+[章节回集卷部篇][\s　]*[^\n]*"
    ).expect("Invalid regex")
});

// Alternative Chinese patterns (e.g., "第1章", "Chapter 1")
static CN_ALT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?m)^[\s　]*(?:第[\d]+[章节回]|Chapter\s+\d+|CHAPTER\s+\d+)[\s　]*[^\n]*"
    ).expect("Invalid regex")
});

// Volume/Part markers
static VOLUME_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?m)^[\s　]*(?:第[一二三四五六七八九十百千万零〇\d]+[卷部]|[卷部][一二三四五六七八九十百千万零〇\d]+|Volume\s+\d+|Part\s+\d+)[\s　]*[^\n]*"
    ).expect("Invalid regex")
});

// Separator-based splitting (lines of ===, ---, ***)
static SEPARATOR_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^[\s　]*[=\-*]{3,}[\s　]*$").expect("Invalid regex")
});

impl ChapterSplitter {
    pub fn new(config: ChunkingConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self {
            config: ChunkingConfig::default(),
        }
    }

    /// Split text into chapters using regex-based detection.
    /// Falls back to fixed-size splitting if no chapter markers are found.
    pub fn split(&self, text: &str) -> Vec<DetectedChapter> {
        // Try Chinese chapter patterns first
        let chapters = self.split_by_pattern(text, &CN_CHAPTER_PATTERN);
        if chapters.len() > 1 {
            return chapters;
        }

        // Try alternative patterns
        let chapters = self.split_by_pattern(text, &CN_ALT_PATTERN);
        if chapters.len() > 1 {
            return chapters;
        }

        // Try volume/part markers when chapter markers are absent
        let chapters = self.split_by_pattern(text, &VOLUME_PATTERN);
        if chapters.len() > 1 {
            return chapters;
        }

        // Try separator-based splitting
        let chapters = self.split_by_pattern(text, &SEPARATOR_PATTERN);
        if chapters.len() > 1 {
            return chapters;
        }

        // Fallback: split into fixed-size chunks
        self.split_by_size(text)
    }

    /// Split text at regex match boundaries.
    fn split_by_pattern(&self, text: &str, pattern: &Regex) -> Vec<DetectedChapter> {
        let matches: Vec<_> = pattern.find_iter(text).collect();

        if matches.is_empty() {
            return vec![DetectedChapter {
                index: 0,
                title: None,
                content: text.to_string(),
                start_offset: 0,
                end_offset: text.len(),
                word_count: text.chars().count() as i32,
            }];
        }

        let mut chapters = Vec::with_capacity(matches.len());

        for (i, m) in matches.iter().enumerate() {
            let start = m.start();
            let end = if i + 1 < matches.len() {
                matches[i + 1].start()
            } else {
                text.len()
            };

            let title = m.as_str().trim().to_string();
            let content = text[start..end].to_string();
            let word_count = content.chars().count() as i32;

            chapters.push(DetectedChapter {
                index: i as i32,
                title: Some(title),
                content,
                start_offset: start,
                end_offset: end,
                word_count,
            });
        }

        // If there's content before the first chapter marker, prepend it
        if !matches.is_empty() && matches[0].start() > 0 {
            let preface = text[..matches[0].start()].trim();
            if !preface.is_empty() && preface.chars().count() >= self.config.min_chunk_size {
                chapters.insert(
                    0,
                    DetectedChapter {
                        index: -1, // Will be renumbered
                        title: Some("序言".to_string()),
                        content: preface.to_string(),
                        start_offset: 0,
                        end_offset: matches[0].start(),
                        word_count: preface.chars().count() as i32,
                    },
                );
            }
        }

        // Renumber chapters
        for (i, ch) in chapters.iter_mut().enumerate() {
            ch.index = i as i32;
        }

        chapters
    }

    /// Split by approximate character count (fallback).
    fn split_by_size(&self, text: &str) -> Vec<DetectedChapter> {
        let target_size = self
            .config
            .chunk_size
            .saturating_mul(10)
            .max(self.config.min_chunk_size)
            .max(1000);
        let mut chapters = Vec::new();
        let mut start = 0;
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let total = chars.len();

        while start < total {
            let mut end = (start + target_size).min(total);

            // Try to break at paragraph boundary
            if end < total {
                // Look for newline within 20% of target
                let search_start = end.saturating_sub(target_size / 5);
                if let Some(pos) = find_paragraph_break(&chars, search_start, end) {
                    end = pos;
                }
            }

            let start_offset = chars[start].0;
            let end_offset = if end < total {
                chars[end].0
            } else {
                text.len()
            };
            let content = text[start_offset..end_offset].to_string();
            chapters.push(DetectedChapter {
                index: chapters.len() as i32,
                title: None,
                content,
                start_offset,
                end_offset,
                word_count: (end - start) as i32,
            });

            start = end;
        }

        chapters
    }
}

fn find_paragraph_break(chars: &[(usize, char)], search_start: usize, end: usize) -> Option<usize> {
    for idx in (search_start + 1..end).rev() {
        if chars[idx - 1].1 == '\n' && chars[idx].1 == '\n' {
            return Some(idx + 1);
        }
    }

    for idx in (search_start..end).rev() {
        if chars[idx].1 == '\n' {
            return Some(idx + 1);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chinese_chapter_splitting() {
        let text = r#"
第一章 初入修真界

少年站在山脚下，仰望着云雾缭绕的山巅。

第二章 机缘巧合

一本古老的秘籍从天而降。

第三章 突破境界

经过三天三夜的苦修，他终于突破了。
"#;

        let splitter = ChapterSplitter::with_defaults();
        let chapters = splitter.split(text);

        assert_eq!(chapters.len(), 3);
        assert!(chapters[0].title.as_ref().map_or(false, |t| t.contains("第一章")));
        assert!(chapters[1].title.as_ref().map_or(false, |t| t.contains("第二章")));
        assert!(chapters[2].title.as_ref().map_or(false, |t| t.contains("第三章")));
    }

    #[test]
    fn test_fallback_size_splitting() {
        let text = "a".repeat(15000);
        let splitter = ChapterSplitter::with_defaults();
        let chapters = splitter.split(&text);

        assert!(chapters.len() >= 2);
    }

    #[test]
    fn test_english_chapter_splitting() {
        let text = r#"
Chapter 1: The Beginning

It was a dark and stormy night.

Chapter 2: The Journey

They set off at dawn.

Chapter 3: The End

Everything came to a close.
"#;
        let splitter = ChapterSplitter::with_defaults();
        let chapters = splitter.split(text);

        assert_eq!(chapters.len(), 3);
        assert!(chapters[0].title.as_ref().map_or(false, |t| t.contains("Chapter 1")));
    }

    #[test]
    fn test_numeric_chapter_patterns() {
        let text = "第1章 开端\n\n内容一\n\n第2章 发展\n\n内容二\n\n第3章 结尾\n\n内容三";
        let splitter = ChapterSplitter::with_defaults();
        let chapters = splitter.split(text);

        assert_eq!(chapters.len(), 3);
    }

    #[test]
    fn test_chapter_content_preserved() {
        let text = "第一章 开头\n\n这是第一章的全部内容，\n包含多行文字。\n\n第二章 中间\n\n第二章内容。";
        let splitter = ChapterSplitter::with_defaults();
        let chapters = splitter.split(text);

        assert!(chapters[0].content.contains("这是第一章的全部内容"));
        assert!(chapters[0].content.contains("包含多行文字"));
    }

    #[test]
    fn test_empty_text() {
        let splitter = ChapterSplitter::with_defaults();
        let chapters = splitter.split("");
        // Should return at least one "chapter" or empty vec
        assert!(chapters.is_empty() || chapters.len() == 1);
    }

    #[test]
    fn test_no_chapter_markers() {
        let text = "这是一段没有章节标记的纯文本。它很短，不需要拆分。";
        let splitter = ChapterSplitter::with_defaults();
        let chapters = splitter.split(text);

        assert_eq!(chapters.len(), 1);
        assert!(chapters[0].content.contains("没有章节标记"));
    }

    #[test]
    fn test_volume_and_chapter_mixed() {
        let text = "卷一 起始篇\n\n第一章 少年\n\n内容一\n\n第二章 旅途\n\n内容二";
        let splitter = ChapterSplitter::with_defaults();
        let chapters = splitter.split(text);

        // Should detect chapters, volume headers may be treated differently
        assert!(chapters.len() >= 2);
    }
}
