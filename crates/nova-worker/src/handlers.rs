use async_trait::async_trait;
use nova_core::{domain::task::TaskKind, Result};
use nova_embed::chunker_v2::{ChunkingConfigV2, NovelChunker};
use serde_json::Value;
use sqlx::PgPool;
use tracing::{info, warn};

use super::TaskHandler;

/// Handles full book ingestion pipeline:
/// 1. Parse file → extract chapters
/// 2. Clean text (remove watermarks, ads)
/// 3. Chunk chapters for embedding
/// 4. Generate embeddings
/// 5. Index in search (Meilisearch + Qdrant)
/// 6. Enqueue entity extraction tasks
pub struct IngestFileHandler {
    db: PgPool,
    meili_url: String,
    meili_key: String,
}

impl IngestFileHandler {
    pub fn new(db: PgPool, meili_url: String, meili_key: String) -> Self {
        Self { db, meili_url, meili_key }
    }
}

#[async_trait]
impl TaskHandler for IngestFileHandler {
    fn task_kind(&self) -> TaskKind {
        TaskKind::ParseFile
    }

    async fn handle(&self, payload: Value) -> Result<Value> {
        let file_path = payload["file_path"]
            .as_str()
            .ok_or_else(|| nova_core::Error::Validation("missing file_path".into()))?;
        let library_id = payload["library_id"]
            .as_str()
            .ok_or_else(|| nova_core::Error::Validation("missing library_id".into()))?;
        let library_uuid = uuid::Uuid::parse_str(library_id).map_err(|error| {
            nova_core::Error::Validation(format!("invalid library_id: {error}"))
        })?;
        let book_id = payload["book_id"]
            .as_str()
            .map(|s| s.to_string());

        info!(file_path, library_id, "Starting file ingestion");

        let path = std::path::Path::new(file_path);
        if !path.exists() {
            return Err(nova_core::Error::NotFound {
                entity: "file",
                id: file_path.to_string(),
            });
        }

        // Step 1: Hash file for deduplication
        let hash = nova_ingest::hasher::hash_file(path).await?;

        // Check if we already have this file (deduplication)
        let existing = sqlx::query_scalar!(
            "SELECT id FROM books WHERE file_hash = $1",
            &hash,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(|e| nova_core::Error::Internal(format!("DB error: {e}")))?;

        if let Some(existing_id) = existing {
            info!(hash = %hash, existing_id = %existing_id, "Duplicate file detected, skipping");
            return Ok(serde_json::json!({
                "status": "skipped",
                "reason": "duplicate",
                "existing_book_id": existing_id.to_string(),
            }));
        }

        // Step 2: Parse document with Kreuzberg
        let doc = nova_ingest::DocumentParser::parse(path).await?;

        if doc.content.trim().is_empty() {
            return Err(nova_core::Error::Parse("Document has no extractable content".into()));
        }

        // Step 3: Split into chapters
        let splitter = nova_ingest::ChapterSplitter::with_defaults();
        let chapters = splitter.split(&doc.content);

        if chapters.is_empty() {
            return Err(nova_core::Error::Parse("No chapters could be extracted".into()));
        }

        // Step 4: Clean each chapter
        let cleaned_chapters: Vec<(String, String)> = chapters.into_iter().map(|ch| {
            let cleaned = nova_ingest::cleaner::TextCleaner::clean(&ch.content);
            (ch.title.unwrap_or_default(), cleaned)
        }).collect();

        // Step 5: Insert book and chapters in a transaction
        let new_book_id = if let Some(id_str) = &book_id {
            uuid::Uuid::parse_str(id_str)
                .map_err(|e| nova_core::Error::Validation(format!("invalid book_id: {e}")))?
        } else {
            uuid::Uuid::new_v4()
        };

        let total_words: i64 = cleaned_chapters.iter()
            .map(|(_, content)| count_words_simple(content))
            .sum();

        let mut tx = self.db.begin().await
            .map_err(|e| nova_core::Error::Internal(format!("Failed to begin transaction: {e}")))?;

        // Acquire every dedup-sensitive lock before the book/chapter rows. The
        // transaction can then wait safely and all row-level triggers reacquire
        // the barriers reentrantly while the whole import stays atomic.
        sqlx::query("SELECT lock_novel_dedup_global_barrier()")
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                nova_core::Error::Internal(format!("Failed to lock import barrier: {error}"))
            })?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(library_uuid.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                nova_core::Error::Internal(format!("Failed to lock library import scope: {error}"))
            })?;
        sqlx::query("SELECT lock_novel_dedup_books($1)")
            .bind(vec![new_book_id])
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                nova_core::Error::Internal(format!("Failed to lock imported book: {error}"))
            })?;

        sqlx::query!(
            r#"INSERT INTO books (id, title, author, language, format, word_count, file_path, file_hash, library_id, status)
               VALUES ($1, $2, $3, $4::language, $5::book_format, $6, $7, $8, $9::uuid, 'available')
               ON CONFLICT (id) DO UPDATE
               SET word_count = $6, file_hash = $8, updated_at = NOW()"#,
            new_book_id,
            doc.title,
            doc.author,
            format!("{:?}", doc.language).to_lowercase() as _,
            format!("{:?}", doc.format).to_lowercase() as _,
            total_words,
            file_path,
            hash,
            library_uuid,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| nova_core::Error::Internal(format!("Failed to insert book: {e}")))?;

        // Step 6: Insert chapters
        for (idx, (title, content)) in cleaned_chapters.iter().enumerate() {
            let chapter_id = uuid::Uuid::new_v4();
            let word_count = count_words_simple(content) as i32;
            sqlx::query!(
                r#"INSERT INTO chapters (id, book_id, "index", chapter_index, title, content, word_count)
                   VALUES ($1, $2, $3, $3, $4, $5, $6)
                   ON CONFLICT (book_id, "index") DO UPDATE
                   SET content = $5, word_count = $6, title = $4, updated_at = NOW()"#,
                chapter_id,
                new_book_id,
                idx as i32,
                title,
                content,
                word_count,
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| nova_core::Error::Internal(format!("Failed to insert chapter {}: {e}", idx)))?;
        }

        tx.commit().await
            .map_err(|e| nova_core::Error::Internal(format!("Failed to commit transaction: {e}")))?;

        // Step 7: Index chunks in Meilisearch using NovelChunker v2
        let chunker = NovelChunker::new(ChunkingConfigV2::default());
        let mut search_documents: Vec<serde_json::Value> = Vec::new();

        for (idx, (title, content)) in cleaned_chapters.iter().enumerate() {
            let v2_chunks = chunker.chunk_document(content, Some(&doc.title));
            for chunk in &v2_chunks {
                search_documents.push(serde_json::json!({
                    "id": format!("{}_{}_{}", new_book_id, idx, chunk.index),
                    "book_id": new_book_id.to_string(),
                    "book_title": doc.title,
                    "chapter_title": title,
                    "chapter_index": idx,
                    "chunk_index": chunk.index,
                    "content": chunk.content,
                    "is_dialogue": chunk.is_dialogue,
                }));
            }
        }

        // Best-effort: index in Meilisearch (non-fatal if unavailable)
        let client = reqwest::Client::new();
        match client
            .post(format!("{}/indexes/chunks/documents", self.meili_url))
            .header("Authorization", format!("Bearer {}", self.meili_key))
            .json(&search_documents)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                info!(book_id = %new_book_id, "Indexed {} chunks in Meilisearch", search_documents.len());
            }
            Ok(resp) => {
                warn!(book_id = %new_book_id, status = %resp.status(), "Meilisearch indexing failed (non-fatal)");
            }
            Err(e) => {
                warn!(book_id = %new_book_id, error = %e, "Meilisearch unreachable (non-fatal)");
            }
        }

        // Step 8: Generate cover if not exists
        if let Err(e) = nova_ingest::extract_cover(path, new_book_id, std::path::Path::new("./data/covers")).await {
            info!(book_id = %new_book_id, "No cover extracted, will generate: {e}");
            // Cover generation for TXT files happens asynchronously via cover.rs
        }

        info!(
            book_id = %new_book_id,
            chapters = cleaned_chapters.len(),
            words = total_words,
            "File ingestion completed successfully"
        );

        Ok(serde_json::json!({
            "status": "completed",
            "book_id": new_book_id.to_string(),
            "chapters_found": cleaned_chapters.len(),
            "word_count": total_words,
            "format": format!("{:?}", doc.format),
            "language": format!("{:?}", doc.language),
        }))
    }

    fn max_retries(&self) -> i32 {
        2
    }
}

/// Simple word count helper for CJK + Western text.
fn count_words_simple(text: &str) -> i64 {
    let mut count: i64 = 0;
    let mut in_word = false;
    for ch in text.chars() {
        if ch as u32 >= 0x4E00 && ch as u32 <= 0x9FFF {
            // CJK character = 1 word
            count += 1;
            in_word = false;
        } else if ch.is_alphanumeric() {
            if !in_word {
                count += 1;
                in_word = true;
            }
        } else {
            in_word = false;
        }
    }
    count
}

/// Handles embedding generation for text chunks
pub struct EmbedChunksHandler {
    db: PgPool,
    embedding_endpoint: String,
}

impl EmbedChunksHandler {
    pub fn new(db: PgPool, embedding_endpoint: String) -> Self {
        Self { db, embedding_endpoint }
    }
}

#[async_trait]
impl TaskHandler for EmbedChunksHandler {
    fn task_kind(&self) -> TaskKind {
        TaskKind::GenerateEmbeddings
    }

    async fn handle(&self, payload: Value) -> Result<Value> {
        let book_id = payload["book_id"]
            .as_str()
            .ok_or_else(|| nova_core::Error::Validation("missing book_id".into()))?;
        let book_uuid = uuid::Uuid::parse_str(book_id)
            .map_err(|e| nova_core::Error::Validation(format!("invalid book_id: {e}")))?;

        info!(book_id, "Generating embeddings for book chapters");

        // Fetch chapter content for embedding
        let chapters = sqlx::query!(
            r#"SELECT id, "index" as chapter_index, content
               FROM chapters
               WHERE book_id = $1
               ORDER BY "index""#,
            book_uuid,
        )
        .fetch_all(&self.db)
        .await
        .map_err(|e| nova_core::Error::Internal(format!("DB error: {e}")))?;

        if chapters.is_empty() {
            return Ok(serde_json::json!({
                "status": "completed",
                "chunks_embedded": 0,
                "reason": "no chapters found",
            }));
        }

        // Chunk chapters using NovelChunker v2 (semantic, dialogue-aware)
        let chunker = NovelChunker::new(ChunkingConfigV2::default());
        let mut chunks: Vec<(uuid::Uuid, i32, usize, String)> = Vec::new();

        // Get book title for context
        let book_title: Option<String> = sqlx::query_scalar!(
            r#"SELECT title FROM books WHERE id = $1"#,
            book_uuid,
        )
        .fetch_optional(&self.db)
        .await
        .ok()
        .flatten();

        for ch in &chapters {
            let content = &ch.content;
            let v2_chunks = chunker.chunk_document(content, book_title.as_deref());

            for chunk in v2_chunks {
                if chunk.content.trim().len() > 50 {
                    chunks.push((ch.id, ch.chapter_index, chunk.index, chunk.content));
                }
            }
        }

        let client = reqwest::Client::new();
        let batch_size = 32;
        let mut embedded_count = 0;

        for batch in chunks.chunks(batch_size) {
            let texts: Vec<&str> = batch.iter().map(|(_, _, _, text)| text.as_str()).collect();

            let resp = client
                .post(format!("{}/embed", self.embedding_endpoint))
                .json(&serde_json::json!({ "texts": texts }))
                .send()
                .await
                .map_err(|e| nova_core::Error::Internal(format!("Embedding service error: {e}")))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(nova_core::Error::Internal(
                    format!("Embedding service returned {}: {}", status, body)
                ));
            }

            let result: serde_json::Value = resp.json().await
                .map_err(|e| nova_core::Error::Internal(format!("Invalid embedding response: {e}")))?;

            let embeddings = result["embeddings"].as_array()
                .ok_or_else(|| nova_core::Error::Internal("Missing embeddings array in response".into()))?;

            // Store embeddings in text_chunks table
            for (i, (chapter_id, chapter_idx, seg_idx, text)) in batch.iter().enumerate() {
                if let Some(embedding) = embeddings.get(i) {
                    let chunk_id = uuid::Uuid::new_v4();
                    let embedding_vec: Vec<f32> = embedding.as_array()
                        .map(|arr| arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
                        .unwrap_or_default();

                    sqlx::query!(
                        r#"INSERT INTO text_chunks (id, book_id, chapter_id, chapter_index, chunk_index, content, embedding)
                           VALUES ($1, $2, $3, $4, $5, $6, $7)
                           ON CONFLICT (book_id, chapter_index, chunk_index) DO UPDATE
                           SET embedding = $7"#,
                        chunk_id,
                        book_uuid,
                        chapter_id,
                        chapter_idx,
                        *seg_idx as i32,
                        text,
                        &embedding_vec as &[f32],
                    )
                    .execute(&self.db)
                    .await
                    .map_err(|e| nova_core::Error::Internal(format!("Failed to store chunk: {e}")))?;

                    embedded_count += 1;
                }
            }
        }

        info!(book_id, chunks = embedded_count, "Embedding generation completed");

        Ok(serde_json::json!({
            "status": "completed",
            "chunks_embedded": embedded_count,
        }))
    }

    fn max_retries(&self) -> i32 {
        3
    }

    fn retry_delay_ms(&self) -> u64 {
        10_000
    }
}

/// Handles AI entity extraction from a chapter
pub struct ExtractEntitiesHandler {
    db: PgPool,
    deepseek_api_key: String,
    deepseek_base_url: String,
}

impl ExtractEntitiesHandler {
    pub fn new(db: PgPool, deepseek_api_key: String, deepseek_base_url: String) -> Self {
        Self { db, deepseek_api_key, deepseek_base_url }
    }
}

#[async_trait]
impl TaskHandler for ExtractEntitiesHandler {
    fn task_kind(&self) -> TaskKind {
        TaskKind::ExtractEntities
    }

    async fn handle(&self, payload: Value) -> Result<Value> {
        let book_id = payload["book_id"]
            .as_str()
            .ok_or_else(|| nova_core::Error::Validation("missing book_id".into()))?;
        let chapter_index = payload["chapter_index"]
            .as_i64()
            .ok_or_else(|| nova_core::Error::Validation("missing chapter_index".into()))?;
        let book_uuid = uuid::Uuid::parse_str(book_id)
            .map_err(|e| nova_core::Error::Validation(format!("invalid book_id: {e}")))?;

        info!(book_id, chapter_index, "Extracting entities via LLM");

        // Fetch chapter content
        let chapter = sqlx::query_scalar!(
            r#"SELECT content FROM chapters WHERE book_id = $1 AND "index" = $2"#,
            book_uuid,
            chapter_index as i32,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(|e| nova_core::Error::Internal(format!("DB error: {e}")))?
        .ok_or_else(|| nova_core::Error::NotFound {
            entity: "chapter",
            id: format!("book={} index={}", book_id, chapter_index),
        })?;

        // Detect language for prompt selection
        let book_language: Option<String> = sqlx::query_scalar!(
            r#"SELECT language::text FROM books WHERE id = $1"#,
            book_uuid,
        )
        .fetch_optional(&self.db)
        .await
        .ok()
        .flatten()
        .flatten();

        let language_str = book_language.as_deref().unwrap_or("chinese");

        // Truncate very long chapters to avoid token limits
        let text_for_extraction = if chapter.len() > 8000 {
            &chapter[..8000]
        } else {
            &chapter
        };

        // Use language-aware entity extraction prompt
        let prompt_template = nova_graph::entity::get_extraction_prompt(language_str);
        let prompt = prompt_template.replace("{text}", text_for_extraction);

        let base_url = if self.deepseek_base_url.is_empty() {
            "https://api.deepseek.com/v1"
        } else {
            &self.deepseek_base_url
        };

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/chat/completions", base_url))
            .header("Authorization", format!("Bearer {}", self.deepseek_api_key))
            .json(&serde_json::json!({
                "model": "deepseek-chat",
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0.1,
                "max_tokens": 2000,
            }))
            .send()
            .await
            .map_err(|e| nova_core::Error::Internal(format!("DeepSeek API error: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(nova_core::Error::Internal(format!("DeepSeek returned {}", status)));
        }

        let result: serde_json::Value = resp.json().await
            .map_err(|e| nova_core::Error::Internal(format!("Invalid DeepSeek response: {e}")))?;

        let content = result["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("[]");

        // Parse the JSON array from the LLM response
        let entities: Vec<serde_json::Value> = serde_json::from_str(content)
            .or_else(|_| {
                // Try to extract JSON from markdown code blocks
                let stripped = content
                    .trim_start_matches("```json")
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim();
                serde_json::from_str(stripped)
            })
            .unwrap_or_default();

        let mut entities_inserted = 0;
        let relationships_found = 0;

        for entity in &entities {
            let name = entity["name"].as_str().unwrap_or("").trim();
            let entity_type = entity["type"].as_str().unwrap_or("character");
            let description = entity["description"].as_str().unwrap_or("");

            if name.is_empty() || name.len() > 100 {
                continue;
            }

            // Upsert entity (deduplicate by name + book)
            let entity_id = uuid::Uuid::new_v4();
            sqlx::query!(
                r#"INSERT INTO entities (id, name, entity_type, description, book_id)
                   VALUES ($1, $2, $3, $4, $5)
                   ON CONFLICT (name, book_id) DO UPDATE
                   SET description = COALESCE(NULLIF($4, ''), entities.description)"#,
                entity_id,
                name,
                entity_type,
                description,
                book_uuid,
            )
            .execute(&self.db)
            .await
            .map_err(|e| nova_core::Error::Internal(format!("Failed to insert entity: {e}")))?;

            entities_inserted += 1;
        }

        info!(
            book_id,
            chapter_index,
            entities = entities_inserted,
            "Entity extraction completed"
        );

        Ok(serde_json::json!({
            "status": "completed",
            "entities_found": entities_inserted,
            "relationships_found": relationships_found,
        }))
    }

    fn max_retries(&self) -> i32 {
        2
    }

    fn retry_delay_ms(&self) -> u64 {
        15_000
    }
}

/// Handles library directory scanning
pub struct ScanLibraryHandler {
    db: PgPool,
}

impl ScanLibraryHandler {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }
}

#[async_trait]
impl TaskHandler for ScanLibraryHandler {
    fn task_kind(&self) -> TaskKind {
        TaskKind::LibraryScan
    }

    async fn handle(&self, payload: Value) -> Result<Value> {
        let library_id = payload["library_id"]
            .as_str()
            .ok_or_else(|| nova_core::Error::Validation("missing library_id".into()))?;
        let path = payload["path"]
            .as_str()
            .ok_or_else(|| nova_core::Error::Validation("missing path".into()))?;

        info!(library_id, path, "Scanning library directory");

        let root = std::path::Path::new(path);
        if !root.is_dir() {
            return Err(nova_core::Error::Validation(format!("Path is not a directory: {}", path)));
        }

        let supported_extensions = ["txt", "epub", "pdf", "docx", "doc", "md", "html"];
        let mut new_files = 0;
        let mut modified_files = 0;

        // Walk directory recursively
        let mut entries = Vec::new();
        collect_files(root, &supported_extensions, &mut entries);

        let files_found = entries.len();

        for file_path in &entries {
            let path_str = file_path.to_string_lossy().to_string();

            // Hash the file
            let hash = match nova_ingest::hasher::hash_file(file_path).await {
                Ok(h) => h,
                Err(e) => {
                    warn!(path = %path_str, error = %e, "Failed to hash file, skipping");
                    continue;
                }
            };

            // Check if file already exists in DB
            let existing = sqlx::query!(
                "SELECT id, file_hash FROM books WHERE file_path = $1",
                &path_str,
            )
            .fetch_optional(&self.db)
            .await
            .map_err(|e| nova_core::Error::Internal(format!("DB error: {e}")))?;

            match existing {
                None => {
                    // New file — enqueue ingest task
                    let task_id = uuid::Uuid::new_v4();
                    sqlx::query!(
                        r#"INSERT INTO tasks (id, kind, status, payload, priority)
                           VALUES ($1, 'parse_file', 'queued', $2, 'normal')"#,
                        task_id,
                        serde_json::json!({
                            "file_path": path_str,
                            "library_id": library_id,
                        }),
                    )
                    .execute(&self.db)
                    .await
                    .map_err(|e| nova_core::Error::Internal(format!("Failed to enqueue task: {e}")))?;

                    new_files += 1;
                }
                Some(book) => {
                    if book.file_hash.as_str() != hash.as_str() {
                        // File modified — enqueue re-ingest
                        let task_id = uuid::Uuid::new_v4();
                        sqlx::query!(
                            r#"INSERT INTO tasks (id, kind, status, payload, priority)
                               VALUES ($1, 'parse_file', 'queued', $2, 'normal')"#,
                            task_id,
                            serde_json::json!({
                                "file_path": path_str,
                                "library_id": library_id,
                                "book_id": book.id.to_string(),
                            }),
                        )
                        .execute(&self.db)
                        .await
                        .map_err(|e| nova_core::Error::Internal(format!("Failed to enqueue task: {e}")))?;

                        modified_files += 1;
                    }
                }
            }
        }

        // Mark books whose files no longer exist as unavailable
        let existing_paths: Vec<String> = entries.iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        if !existing_paths.is_empty() {
            let lib_uuid = uuid::Uuid::parse_str(library_id).ok();
            sqlx::query!(
                r#"UPDATE books SET status = 'unavailable', updated_at = NOW()
                   WHERE library_id = $1 AND file_path IS NOT NULL
                   AND file_path != ALL($2) AND status = 'available'"#,
                lib_uuid,
                &existing_paths,
            )
            .execute(&self.db)
            .await
            .map_err(|e| nova_core::Error::Internal(format!("Failed to mark missing books: {e}")))?;
        }

        info!(library_id, files_found, new_files, modified_files, "Library scan completed");

        Ok(serde_json::json!({
            "status": "completed",
            "files_found": files_found,
            "new_files": new_files,
            "modified_files": modified_files,
        }))
    }

    fn max_retries(&self) -> i32 {
        1
    }
}

/// Recursively collect files with supported extensions.
fn collect_files(dir: &std::path::Path, extensions: &[&str], out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip hidden directories
            if path.file_name().map(|n| n.to_string_lossy().starts_with('.')).unwrap_or(false) {
                continue;
            }
            collect_files(&path, extensions, out);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if extensions.contains(&ext.to_lowercase().as_str()) {
                out.push(path);
            }
        }
    }
}

/// Handles translation of a chapter
pub struct TranslateChapterHandler {
    db: PgPool,
    deepseek_api_key: String,
    deepseek_base_url: String,
}

impl TranslateChapterHandler {
    pub fn new(db: PgPool, deepseek_api_key: String, deepseek_base_url: String) -> Self {
        Self { db, deepseek_api_key, deepseek_base_url }
    }
}

#[async_trait]
impl TaskHandler for TranslateChapterHandler {
    fn task_kind(&self) -> TaskKind {
        TaskKind::Translate
    }

    async fn handle(&self, payload: Value) -> Result<Value> {
        let book_id = payload["book_id"]
            .as_str()
            .ok_or_else(|| nova_core::Error::Validation("missing book_id".into()))?;
        let chapter_index = payload["chapter_index"]
            .as_i64()
            .ok_or_else(|| nova_core::Error::Validation("missing chapter_index".into()))?;
        let target_language = payload["target_language"]
            .as_str()
            .unwrap_or("en");
        let book_uuid = uuid::Uuid::parse_str(book_id)
            .map_err(|e| nova_core::Error::Validation(format!("invalid book_id: {e}")))?;

        info!(book_id, chapter_index, target_language, "Translating chapter");

        // Load chapter content
        let content = sqlx::query_scalar!(
            r#"SELECT content FROM chapters WHERE book_id = $1 AND "index" = $2"#,
            book_uuid,
            chapter_index as i32,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(|e| nova_core::Error::Internal(format!("DB error: {e}")))?
        .ok_or_else(|| nova_core::Error::NotFound {
            entity: "chapter",
            id: format!("book={} index={}", book_id, chapter_index),
        })?;

        // Load glossary for context
        let glossary = sqlx::query!(
            "SELECT source_term, target_term FROM glossary_entries WHERE book_id = $1 LIMIT 100",
            book_uuid,
        )
        .fetch_all(&self.db)
        .await
        .unwrap_or_default();

        let glossary_context = if glossary.is_empty() {
            String::new()
        } else {
            let entries: Vec<String> = glossary.iter()
                .map(|g| format!("{} → {}", g.source_term, &g.target_term))
                .collect();
            format!("\n\n术语表（必须按此翻译）：\n{}", entries.join("\n"))
        };

        // Split into paragraphs for translation
        let paragraphs: Vec<&str> = content.split("\n\n")
            .filter(|p| !p.trim().is_empty())
            .collect();

        let base_url = if self.deepseek_base_url.is_empty() {
            "https://api.deepseek.com/v1"
        } else {
            &self.deepseek_base_url
        };

        let client = reqwest::Client::new();
        let mut translated_paragraphs = Vec::new();

        // Batch paragraphs to avoid too many API calls (group into ~2000 char batches)
        let mut current_batch = String::new();
        let mut batches: Vec<String> = Vec::new();

        for para in &paragraphs {
            if current_batch.len() + para.len() > 2000 && !current_batch.is_empty() {
                batches.push(current_batch.clone());
                current_batch.clear();
            }
            if !current_batch.is_empty() {
                current_batch.push_str("\n\n");
            }
            current_batch.push_str(para);
        }
        if !current_batch.is_empty() {
            batches.push(current_batch);
        }

        for batch in &batches {
            let prompt = format!(
                "将以下中文小说段落翻译成{}。保持原文风格和语气。不要添加解释。{}\n\n原文：\n{}",
                match target_language {
                    "en" => "英文",
                    "ja" => "日文",
                    "ko" => "韩文",
                    _ => target_language,
                },
                glossary_context,
                batch,
            );

            let resp = client
                .post(format!("{}/chat/completions", base_url))
                .header("Authorization", format!("Bearer {}", self.deepseek_api_key))
                .json(&serde_json::json!({
                    "model": "deepseek-chat",
                    "messages": [{"role": "user", "content": prompt}],
                    "temperature": 0.3,
                    "max_tokens": 4000,
                }))
                .send()
                .await
                .map_err(|e| nova_core::Error::Internal(format!("DeepSeek API error: {e}")))?;

            if !resp.status().is_success() {
                let status = resp.status();
                return Err(nova_core::Error::Internal(format!("DeepSeek returned {}", status)));
            }

            let result: serde_json::Value = resp.json().await
                .map_err(|e| nova_core::Error::Internal(format!("Invalid response: {e}")))?;

            let translated = result["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .to_string();

            translated_paragraphs.push(translated);
        }

        let full_translation = translated_paragraphs.join("\n\n");

        // Store translation in DB
        let translation_id = uuid::Uuid::new_v4();
        sqlx::query!(
            r#"INSERT INTO translations (id, book_id, chapter_index, target_language, content)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT (book_id, chapter_index, target_language) DO UPDATE
               SET content = $5, updated_at = NOW()"#,
            translation_id,
            book_uuid,
            chapter_index as i32,
            target_language,
            full_translation,
        )
        .execute(&self.db)
        .await
        .map_err(|e| nova_core::Error::Internal(format!("Failed to store translation: {e}")))?;

        info!(book_id, chapter_index, target_language, segments = batches.len(), "Translation completed");

        Ok(serde_json::json!({
            "status": "completed",
            "segments_translated": batches.len(),
            "target_language": target_language,
        }))
    }

    fn max_retries(&self) -> i32 {
        2
    }

    fn retry_delay_ms(&self) -> u64 {
        20_000
    }
}
