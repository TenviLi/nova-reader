use regex::Regex;
use std::sync::LazyLock;

/// Text cleaner for removing common noise from web novel sources.
/// Handles Discuz forum artifacts, watermarks, and advertisements.
pub struct TextCleaner;

// Common forum signatures and ads
static FORUM_SIGNATURE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ms)^[\s　]*[-=]{3,}[\s　]*\n.{0,500}(?:签名|广告|本帖|发表于|楼主|回复|Discuz|论坛|下载地址|支持正版).{0,500}\n[\s　]*[-=]{3,}[\s　]*$"
    ).expect("Invalid regex")
});

// Anti-piracy watermarks (common pattern: random text inserted mid-paragraph)
static WATERMARK_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:本文首发|首发网址|正版阅读|防盗章节|盗版必究|请到[\S]+阅读|手打吧|笔趣阁|起点中文网|纵横中文网)[^\n]*"
    ).expect("Invalid regex")
});

// URL patterns
static URL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"https?://[^\s\n]+").expect("Invalid regex")
});

// Excessive whitespace/blank lines
static EXCESS_NEWLINES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\n{4,}").expect("Invalid regex")
});

// Unicode garbage/control characters (except common whitespace)
static CONTROL_CHARS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\x00-\x08\x0B\x0C\x0E-\x1F\x7F]").expect("Invalid regex")
});

impl TextCleaner {
    /// Apply all cleaning rules to the text.
    pub fn clean(text: &str) -> String {
        let mut result = text.to_string();

        // Remove control characters
        result = CONTROL_CHARS.replace_all(&result, "").to_string();

        // Remove bounded forum signature/ad blocks
        result = FORUM_SIGNATURE.replace_all(&result, "").to_string();

        // Remove watermark insertions
        result = WATERMARK_PATTERN.replace_all(&result, "").to_string();

        // Remove URLs
        result = URL_PATTERN.replace_all(&result, "").to_string();

        // Collapse excessive blank lines
        result = EXCESS_NEWLINES.replace_all(&result, "\n\n\n").to_string();

        // Trim leading/trailing whitespace from each line
        result = result
            .lines()
            .map(|line| line.trim())
            .collect::<Vec<_>>()
            .join("\n");

        // Final trim
        result.trim().to_string()
    }

    /// Detect if text likely contains significant noise.
    /// Returns a noise score from 0.0 (clean) to 1.0 (very noisy).
    pub fn noise_score(text: &str) -> f64 {
        let total_len = text.len() as f64;
        if total_len == 0.0 {
            return 0.0;
        }

        let mut noise_chars = 0usize;

        // Count bounded forum signature/ad blocks
        for m in FORUM_SIGNATURE.find_iter(text) {
            noise_chars += m.len();
        }

        // Count watermark matches
        for m in WATERMARK_PATTERN.find_iter(text) {
            noise_chars += m.len();
        }

        // Count URLs
        for m in URL_PATTERN.find_iter(text) {
            noise_chars += m.len();
        }

        // Count control characters
        for m in CONTROL_CHARS.find_iter(text) {
            noise_chars += m.len();
        }

        (noise_chars as f64 / total_len).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watermark_removal() {
        let text = "这是正文内容。\n本文首发于笔趣阁，请支持正版\n继续阅读正文。";
        let cleaned = TextCleaner::clean(text);
        assert!(!cleaned.contains("笔趣阁"));
        assert!(cleaned.contains("这是正文内容"));
        assert!(cleaned.contains("继续阅读正文"));
    }

    #[test]
    fn test_url_removal() {
        let text = "正文内容 https://example.com/piracy 继续";
        let cleaned = TextCleaner::clean(text);
        assert!(!cleaned.contains("https://"));
    }

    #[test]
    fn test_forum_signature_removal() {
        let text = "正文开始。\n---\n本帖由论坛自动生成签名\n下载地址请访问示例站\n---\n正文继续。";
        let cleaned = TextCleaner::clean(text);
        assert!(!cleaned.contains("自动生成签名"));
        assert!(cleaned.contains("正文开始"));
        assert!(cleaned.contains("正文继续"));
    }

    #[test]
    fn test_noise_score() {
        let clean = "这是一段很干净的小说正文，没有任何广告或水印。";
        let noisy = "本文首发于笔趣阁 请到笔趣阁阅读 正版阅读请到起点中文网";

        assert!(TextCleaner::noise_score(clean) < 0.1);
        assert!(TextCleaner::noise_score(noisy) > 0.3);
    }
}
