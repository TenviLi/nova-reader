//! Cover art extraction from various ebook formats.
//! Uses Kreuzberg's extraction capabilities + manual EPUB cover detection.

use std::path::Path;
use anyhow::Result;
use tokio::fs;
use uuid::Uuid;

/// Attempt to extract cover image from a book file.
/// Returns the path where the cover was saved, if successful.
pub async fn extract_cover(file_path: &Path, book_id: Uuid, covers_dir: &Path) -> Result<Option<String>> {
    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let cover_data = match ext.as_str() {
        "epub" => extract_epub_cover(file_path).await?,
        "pdf" => extract_pdf_cover(file_path).await?,
        "txt" | "md" | "html" => {
            // Generate a gradient cover with title/author for plain text formats
            return generate_text_cover(file_path, book_id, covers_dir).await;
        }
        _ => None,
    };

    if let Some(data) = cover_data {
        // Determine format from magic bytes
        let format = detect_image_format(&data).unwrap_or("jpg");
        let filename = format!("{}.{}", book_id, format);
        let cover_path = covers_dir.join(&filename);

        // Ensure covers directory exists
        fs::create_dir_all(covers_dir).await?;
        fs::write(&cover_path, &data).await?;

        Ok(Some(format!("/api/covers/{}", filename)))
    } else {
        Ok(None)
    }
}

/// Extract cover from EPUB (look for cover image in OPF metadata)
async fn extract_epub_cover(path: &Path) -> Result<Option<Vec<u8>>> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    // Strategy 1: Look for meta name="cover" in OPF
    let mut opf_path = None;
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if name.ends_with(".opf") {
            opf_path = Some(name);
            break;
        }
    }

    if let Some(opf) = opf_path {
        let mut opf_content = String::new();
        archive.by_name(&opf)?.read_to_string(&mut opf_content)?;

        // Parse OPF for cover image reference
        if let Some(cover_id) = find_cover_id_in_opf(&opf_content) {
            if let Some(cover_href) = find_href_by_id(&opf_content, &cover_id) {
                // Resolve relative path
                let opf_dir = Path::new(&opf).parent().unwrap_or(Path::new(""));
                let cover_full_path = opf_dir.join(&cover_href).to_string_lossy().to_string();

                if let Ok(mut file) = archive.by_name(&cover_full_path) {
                    let mut buf = Vec::new();
                    file.read_to_end(&mut buf)?;
                    return Ok(Some(buf));
                }
            }
        }
    }

    // Strategy 2: Look for common cover filenames
    let cover_names = ["cover.jpg", "cover.jpeg", "cover.png", "Images/cover.jpg", "OEBPS/Images/cover.jpg"];
    for name in &cover_names {
        if let Ok(mut file) = archive.by_name(name) {
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            return Ok(Some(buf));
        }
    }

    Ok(None)
}

/// Extract first page as cover from PDF (placeholder — requires external tool)
async fn extract_pdf_cover(_path: &Path) -> Result<Option<Vec<u8>>> {
    // PDF cover extraction would require a rendering library
    // For now, return None — Kreuzberg can potentially handle this
    Ok(None)
}

/// Generate an SVG-based cover for text files that lack embedded cover art.
/// Produces a visually appealing gradient + title + author composition.
/// Design: inspired by Penguin Classics / Vintage Books aesthetics.
async fn generate_text_cover(file_path: &Path, book_id: Uuid, covers_dir: &Path) -> Result<Option<String>> {
    let filename_stem = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("未知书名");

    // Extract title and author from filename pattern: "Author - Title" or just "Title"
    let (title, author) = if let Some(idx) = filename_stem.find(" - ") {
        (&filename_stem[idx + 3..], Some(&filename_stem[..idx]))
    } else {
        (filename_stem, None)
    };

    // Deterministic palette from title hash
    let hash = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        title.hash(&mut hasher);
        hasher.finish()
    };

    // Curated color palettes (warm literary tones)
    let palettes: &[(u16, u16, u16, u16)] = &[
        // (hue1, sat1, hue2, sat2)
        (220, 40, 250, 35),  // deep blue → purple
        (340, 45, 20, 40),   // burgundy → warm red
        (160, 35, 190, 30),  // teal → ocean blue
        (30, 50, 45, 45),    // amber → gold
        (270, 35, 300, 40),  // violet → magenta
        (180, 30, 210, 35),  // cyan → navy
        (15, 55, 35, 50),    // burnt orange → terracotta
        (200, 40, 230, 45),  // steel blue → indigo
    ];
    let palette = palettes[(hash % palettes.len() as u64) as usize];
    let (hue1, sat1, hue2, sat2) = palette;

    // Decorative pattern variant
    let pattern_variant = (hash / 8) % 4;

    let author_element = if let Some(a) = author {
        format!(
            r#"<text x="200" y="470" text-anchor="middle" font-family="'Georgia', serif" font-size="18" font-style="italic" fill="rgba(255,255,255,0.75)" letter-spacing="1">{}</text>"#,
            escape_xml(a)
        )
    } else {
        String::new()
    };

    // Multi-line title wrapping (max ~14 CJK chars or ~20 latin chars per line)
    let title_lines = wrap_title(title, 14);
    let title_start_y = 280 - ((title_lines.len() as i32 - 1) * 20);
    let title_elements: String = title_lines.iter().enumerate().map(|(i, line)| {
        let y = title_start_y + (i as i32 * 44);
        format!(
            r#"<text x="200" y="{y}" text-anchor="middle" font-family="'Georgia', 'Noto Serif SC', serif" font-weight="700" font-size="30" fill="white" letter-spacing="0.5">{}</text>"#,
            escape_xml(line)
        )
    }).collect::<Vec<_>>().join("\n  ");

    // Decorative separator line
    let separator_y = title_start_y + (title_lines.len() as i32 * 44) + 20;

    // Subtle pattern overlay
    let pattern_overlay = match pattern_variant {
        0 => format!(
            r#"<circle cx="200" cy="100" r="60" fill="none" stroke="rgba(255,255,255,0.06)" stroke-width="1"/>
  <circle cx="200" cy="100" r="40" fill="none" stroke="rgba(255,255,255,0.04)" stroke-width="1"/>"#
        ),
        1 => format!(
            r#"<line x1="50" y1="80" x2="350" y2="80" stroke="rgba(255,255,255,0.08)" stroke-width="0.5"/>
  <line x1="50" y1="520" x2="350" y2="520" stroke="rgba(255,255,255,0.08)" stroke-width="0.5"/>"#
        ),
        2 => format!(
            r#"<rect x="170" y="70" width="60" height="60" rx="30" fill="none" stroke="rgba(255,255,255,0.07)" stroke-width="1"/>"#
        ),
        _ => format!(
            r#"<path d="M 180 90 L 200 70 L 220 90" fill="none" stroke="rgba(255,255,255,0.08)" stroke-width="1"/>"#
        ),
    };

    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="600" viewBox="0 0 400 600">
  <defs>
    <linearGradient id="bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" style="stop-color:hsl({hue1},{sat1}%,22%)"/>
      <stop offset="50%" style="stop-color:hsl({hue_mid},{sat_mid}%,18%)"/>
      <stop offset="100%" style="stop-color:hsl({hue2},{sat2}%,12%)"/>
    </linearGradient>
    <filter id="noise">
      <feTurbulence type="fractalNoise" baseFrequency="0.7" numOctaves="3" result="noise"/>
      <feColorMatrix type="saturate" values="0" in="noise" result="gray"/>
      <feBlend in="SourceGraphic" in2="gray" mode="overlay" result="blended"/>
      <feComponentTransfer in="blended">
        <feFuncA type="linear" slope="0.03"/>
      </feComponentTransfer>
    </filter>
  </defs>
  <rect width="400" height="600" fill="url(#bg)"/>
  <rect width="400" height="600" filter="url(#noise)" opacity="0.3"/>
  <rect x="24" y="24" width="352" height="552" rx="4" fill="none" stroke="rgba(255,255,255,0.1)" stroke-width="0.5"/>
  {pattern_overlay}
  {title_elements}
  <line x1="150" y1="{separator_y}" x2="250" y2="{separator_y}" stroke="rgba(255,255,255,0.3)" stroke-width="0.8"/>
  {author_element}
  <text x="200" y="560" text-anchor="middle" font-family="'SF Mono', 'JetBrains Mono', monospace" font-size="9" fill="rgba(255,255,255,0.25)" letter-spacing="3">NOVA READER</text>
</svg>"#,
        hue1 = hue1,
        sat1 = sat1,
        hue2 = hue2,
        sat2 = sat2,
        hue_mid = (hue1 + hue2) / 2,
        sat_mid = (sat1 + sat2) / 2,
        pattern_overlay = pattern_overlay,
        title_elements = title_elements,
        separator_y = separator_y,
        author_element = author_element,
    );

    // Save as SVG
    let cover_filename = format!("{}.svg", book_id);
    let cover_path = covers_dir.join(&cover_filename);
    fs::create_dir_all(covers_dir).await?;
    fs::write(&cover_path, svg.as_bytes()).await?;

    Ok(Some(format!("/api/covers/{}", cover_filename)))
}

/// Wrap a title into lines of approximately `max_chars` width.
/// Handles both CJK and Latin text intelligently.
fn wrap_title(title: &str, max_chars: usize) -> Vec<String> {
    let chars: Vec<char> = title.chars().collect();
    if chars.len() <= max_chars {
        return vec![title.to_string()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut char_count = 0;

    for ch in &chars {
        // CJK characters count as ~2 latin chars for width
        let width = if *ch > '\u{2E80}' { 2 } else { 1 };

        if char_count + width > max_chars && !current_line.is_empty() {
            lines.push(current_line.trim().to_string());
            current_line = String::new();
            char_count = 0;
        }
        current_line.push(*ch);
        char_count += width;
    }
    if !current_line.is_empty() {
        lines.push(current_line.trim().to_string());
    }

    // Max 4 lines, truncate with ellipsis
    if lines.len() > 4 {
        lines.truncate(4);
        if let Some(last) = lines.last_mut() {
            last.push('…');
        }
    }
    lines
}

/// Escape XML special characters for safe SVG text embedding
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn find_cover_id_in_opf(opf: &str) -> Option<String> {
    // Look for <meta name="cover" content="cover-image-id"/>
    for line in opf.lines() {
        if line.contains("name=\"cover\"") || line.contains("name='cover'") {
            if let Some(start) = line.find("content=\"").or_else(|| line.find("content='")) {
                let quote = line.as_bytes()[start + 8] as char;
                let rest = &line[start + 9..];
                if let Some(end) = rest.find(quote) {
                    return Some(rest[..end].to_string());
                }
            }
        }
    }
    None
}

fn find_href_by_id(opf: &str, id: &str) -> Option<String> {
    // Look for <item id="cover-image-id" href="..." />
    let search = format!("id=\"{}\"", id);
    for line in opf.lines() {
        if line.contains(&search) {
            if let Some(start) = line.find("href=\"").or_else(|| line.find("href='")) {
                let quote = line.as_bytes()[start + 5] as char;
                let rest = &line[start + 6..];
                if let Some(end) = rest.find(quote) {
                    return Some(rest[..end].to_string());
                }
            }
        }
    }
    None
}

fn detect_image_format(data: &[u8]) -> Option<&'static str> {
    if data.len() < 4 {
        return None;
    }
    match &data[..4] {
        [0xFF, 0xD8, 0xFF, ..] => Some("jpg"),
        [0x89, 0x50, 0x4E, 0x47] => Some("png"),
        [0x47, 0x49, 0x46, 0x38] => Some("gif"),
        [0x52, 0x49, 0x46, 0x46] => Some("webp"), // RIFF header (WebP)
        _ => None,
    }
}

/// Test-accessible wrappers
#[cfg(test)]
pub fn detect_image_format_pub(data: &[u8]) -> Option<&'static str> {
    detect_image_format(data)
}

#[cfg(test)]
pub fn find_cover_id_in_opf_pub(opf: &str) -> Option<String> {
    find_cover_id_in_opf(opf)
}

#[cfg(test)]
pub fn find_href_by_id_pub(opf: &str, id: &str) -> Option<String> {
    find_href_by_id(opf, id)
}
