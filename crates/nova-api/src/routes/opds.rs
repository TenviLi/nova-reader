use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::header,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};

use crate::{error::ApiError, state::AppState};

/// OPDS (Open Publication Distribution System) feed routes
/// Enables integration with external e-reader apps (KOReader, Librera, etc.)
/// v1: Atom XML feeds, v2: JSON (OPDS 2.0 compatible)
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(opds_root))
        .route("/all", get(opds_all_books))
        .route("/recent", get(opds_recent))
        .route("/search", get(opds_search))
        // OPDS 2.0 JSON endpoints
        .route("/v2", get(opds_v2_root))
        .route("/v2/publications", get(opds_v2_publications))
}

const OPDS_CONTENT_TYPE: &str = "application/atom+xml;profile=opds-catalog;kind=navigation";
const OPDS_ACQUISITION_TYPE: &str = "application/atom+xml;profile=opds-catalog;kind=acquisition";

fn xml_response(body: String, content_type: &str) -> Response {
    (
        [(header::CONTENT_TYPE, content_type)],
        body,
    ).into_response()
}

async fn opds_root(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let feed = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>urn:nova-reader:catalog</id>
  <title>Nova Reader 书库</title>
  <updated>{}</updated>
  <author>
    <name>Nova Reader</name>
  </author>
  <link rel="self" href="/opds/" type="{OPDS_CONTENT_TYPE}"/>
  <link rel="start" href="/opds/" type="{OPDS_CONTENT_TYPE}"/>
  <link rel="search" href="/opds/search?q={{searchTerms}}" type="{OPDS_ACQUISITION_TYPE}"/>

  <entry>
    <title>全部书籍</title>
    <id>urn:nova-reader:all</id>
    <link rel="subsection" href="/opds/all" type="{OPDS_ACQUISITION_TYPE}"/>
    <content type="text">浏览所有书籍</content>
  </entry>

  <entry>
    <title>最近添加</title>
    <id>urn:nova-reader:recent</id>
    <link rel="subsection" href="/opds/recent" type="{OPDS_ACQUISITION_TYPE}"/>
    <content type="text">最近添加的书籍</content>
  </entry>
</feed>"#,
        chrono::Utc::now().to_rfc3339()
    );

    xml_response(feed, OPDS_CONTENT_TYPE)
}

async fn opds_all_books(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, ApiError> {
    let books = sqlx::query!(
        r#"
        SELECT id, title, author, language, format, word_count, created_at, updated_at
        FROM books
        ORDER BY title
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    let entries: Vec<String> = books.iter().map(|b| {
        let author_xml = b.author.as_deref().unwrap_or("Unknown");
        let mime = match b.format.as_str() {
            "epub" => "application/epub+zip",
            "pdf" => "application/pdf",
            "txt" => "text/plain",
            _ => "application/octet-stream",
        };
        format!(
            r#"  <entry>
    <title>{}</title>
    <id>urn:nova-reader:book:{}</id>
    <author><name>{}</name></author>
    <updated>{}</updated>
    <link rel="http://opds-spec.org/acquisition" href="/api/books/{}/download" type="{}"/>
    <link rel="http://opds-spec.org/image" href="/api/covers/{}"/>
  </entry>"#,
            xml_escape(&b.title), b.id, xml_escape(author_xml),
            b.updated_at, b.id, mime, b.id,
        )
    }).collect();

    let feed = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>urn:nova-reader:all</id>
  <title>全部书籍</title>
  <updated>{}</updated>
  <link rel="self" href="/opds/all" type="{OPDS_ACQUISITION_TYPE}"/>
  <link rel="start" href="/opds/" type="{OPDS_CONTENT_TYPE}"/>
{}
</feed>"#,
        chrono::Utc::now().to_rfc3339(),
        entries.join("\n"),
    );

    Ok(xml_response(feed, OPDS_ACQUISITION_TYPE))
}

async fn opds_recent(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, ApiError> {
    let books = sqlx::query!(
        r#"
        SELECT id, title, author, format, updated_at
        FROM books
        ORDER BY created_at DESC
        LIMIT 50
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    let entries: Vec<String> = books.iter().map(|b| {
        let author_xml = b.author.as_deref().unwrap_or("Unknown");
        let mime = match b.format.as_str() {
            "epub" => "application/epub+zip",
            "pdf" => "application/pdf",
            _ => "application/octet-stream",
        };
        format!(
            r#"  <entry>
    <title>{}</title>
    <id>urn:nova-reader:book:{}</id>
    <author><name>{}</name></author>
    <updated>{}</updated>
    <link rel="http://opds-spec.org/acquisition" href="/api/books/{}/download" type="{}"/>
  </entry>"#,
            xml_escape(&b.title), b.id, xml_escape(author_xml),
            b.updated_at, b.id, mime,
        )
    }).collect();

    let feed = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>urn:nova-reader:recent</id>
  <title>最近添加</title>
  <updated>{}</updated>
  <link rel="self" href="/opds/recent" type="{OPDS_ACQUISITION_TYPE}"/>
{}
</feed>"#,
        chrono::Utc::now().to_rfc3339(),
        entries.join("\n"),
    );

    Ok(xml_response(feed, OPDS_ACQUISITION_TYPE))
}

async fn opds_search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<OpdsSearchParams>,
) -> Result<impl IntoResponse, ApiError> {
    let query = params.q.unwrap_or_default();
    if query.is_empty() {
        return opds_all_books(State(state)).await;
    }

    // Search books by title/author
    let search_pattern = format!("%{}%", query);
    let books = sqlx::query!(
        r#"SELECT id, title, author, format, updated_at
           FROM books
           WHERE title ILIKE $1 OR author ILIKE $1
           ORDER BY title
           LIMIT 50"#,
        search_pattern,
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    let entries: Vec<String> = books.iter().map(|b| {
        let author_xml = b.author.as_deref().unwrap_or("Unknown");
        let mime = match b.format.as_str() {
            "epub" => "application/epub+zip",
            "pdf" => "application/pdf",
            _ => "application/octet-stream",
        };
        format!(
            r#"  <entry>
    <title>{}</title>
    <id>urn:nova-reader:book:{}</id>
    <author><name>{}</name></author>
    <updated>{}</updated>
    <link rel="http://opds-spec.org/acquisition" href="/api/books/{}/download" type="{}"/>
  </entry>"#,
            xml_escape(&b.title), b.id, xml_escape(author_xml),
            b.updated_at, b.id, mime,
        )
    }).collect();

    let feed = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>urn:nova-reader:search:{}</id>
  <title>搜索: {}</title>
  <updated>{}</updated>
  <link rel="self" href="/opds/search?q={}" type="{OPDS_ACQUISITION_TYPE}"/>
  <link rel="start" href="/opds/" type="{OPDS_CONTENT_TYPE}"/>
{}
</feed>"#,
        xml_escape(&query),
        xml_escape(&query),
        chrono::Utc::now().to_rfc3339(),
        urlencoding::encode(&query),
        entries.join("\n"),
    );

    Ok(xml_response(feed, OPDS_ACQUISITION_TYPE))
}

#[derive(Debug, Deserialize)]
struct OpdsSearchParams {
    q: Option<String>,
}

/// Escape text for safe embedding in XML.
/// Handles XML special characters AND control characters (XML 1.0 forbids most control chars).
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0 valid characters: #x9 | #xA | #xD | [#x20-#xD7FF] | [#xE000-#xFFFD]
            '\x09' | '\x0A' | '\x0D' => out.push(ch),
            c if c < '\x20' => {} // Strip invalid control characters
            c if ('\u{D800}'..='\u{DFFF}').contains(&c) => {} // Strip surrogates
            c if c == '\u{FFFE}' || c == '\u{FFFF}' => {} // Strip non-characters
            c => out.push(c),
        }
    }
    out
}

// ─── OPDS 2.0 (JSON) ────────────────────────────────────────────────────────

#[derive(Serialize)]
struct OpdsV2Feed {
    metadata: OpdsV2Metadata,
    links: Vec<OpdsV2Link>,
    publications: Vec<OpdsV2Publication>,
}

#[derive(Serialize)]
struct OpdsV2Metadata {
    title: String,
    #[serde(rename = "numberOfItems", skip_serializing_if = "Option::is_none")]
    number_of_items: Option<i64>,
}

#[derive(Serialize)]
struct OpdsV2Link {
    rel: String,
    href: String,
    #[serde(rename = "type")]
    media_type: String,
}

#[derive(Serialize)]
struct OpdsV2Publication {
    metadata: OpdsV2PubMetadata,
    links: Vec<OpdsV2Link>,
    images: Vec<OpdsV2Link>,
}

#[derive(Serialize)]
struct OpdsV2PubMetadata {
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    identifier: String,
}

async fn opds_v2_root(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    let feed = OpdsV2Feed {
        metadata: OpdsV2Metadata { title: "Nova Reader OPDS 2.0".to_string(), number_of_items: None },
        links: vec![
            OpdsV2Link { rel: "self".to_string(), href: "/opds/v2".to_string(), media_type: "application/opds+json".to_string() },
            OpdsV2Link { rel: "http://opds-spec.org/shelf".to_string(), href: "/opds/v2/publications".to_string(), media_type: "application/opds+json".to_string() },
        ],
        publications: vec![],
    };

    (
        [(header::CONTENT_TYPE, "application/opds+json")],
        serde_json::to_string(&feed).unwrap_or_default(),
    )
}

async fn opds_v2_publications(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, ApiError> {
    let books = sqlx::query!(
        r#"SELECT id, title, author, language, format, cover_path
           FROM books ORDER BY created_at DESC LIMIT 100"#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    let publications: Vec<OpdsV2Publication> = books.into_iter().map(|b| {
        let links = vec![
            OpdsV2Link {
                rel: "http://opds-spec.org/acquisition".to_string(),
                href: format!("/api/books/{}/download", b.id),
                media_type: match b.format.as_str() {
                    "epub" => "application/epub+zip".to_string(),
                    "pdf" => "application/pdf".to_string(),
                    _ => "application/octet-stream".to_string(),
                },
            },
        ];

        let mut images = vec![];
        if b.cover_path.is_some() {
            images.push(OpdsV2Link {
                rel: "http://opds-spec.org/image".to_string(),
                href: format!("/api/books/{}/cover", b.id),
                media_type: "image/jpeg".to_string(),
            });
        }

        OpdsV2Publication {
            metadata: OpdsV2PubMetadata {
                title: b.title,
                author: b.author,
                language: Some(b.language),
                identifier: format!("urn:uuid:{}", b.id),
            },
            links,
            images,
        }
    }).collect();

    let feed = OpdsV2Feed {
        metadata: OpdsV2Metadata { title: "所有书籍".to_string(), number_of_items: Some(publications.len() as i64) },
        links: vec![
            OpdsV2Link { rel: "self".to_string(), href: "/opds/v2/publications".to_string(), media_type: "application/opds+json".to_string() },
        ],
        publications,
    };

    Ok((
        [(header::CONTENT_TYPE, "application/opds+json")],
        serde_json::to_string(&feed).unwrap_or_default(),
    ))
}
