//! Integration test helpers for running tests with real database connections.
//! Uses test containers pattern: expects a running PostgreSQL for integration tests.
//!
//! Run with: DATABASE_URL=postgres://... cargo test --features integration
//!
//! For CI, the database is provided by a services container in GitHub Actions.

#[path = "../src/migrations.rs"]
mod migrations;

#[cfg(test)]
mod integration {
    use super::migrations;

    static DEDUP_WRITE_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[derive(sqlx::FromRow)]
    struct QueuedAutomaticScan {
        task_id: uuid::Uuid,
        payload: serde_json::Value,
        scheduled_at: chrono::DateTime<chrono::Utc>,
        books_total: i32,
        requested_by: Option<uuid::Uuid>,
        task_book_id: Option<uuid::Uuid>,
    }

    /// Helper to create a test database connection.
    /// Expects DATABASE_URL environment variable.
    async fn test_db() -> Option<sqlx::PgPool> {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            return None;
        };
        let pool = sqlx::PgPool::connect(&url)
            .await
            .expect("DATABASE_URL is set but database connection failed");

        migrations::run_database_migrations(&pool)
            .await
            .expect("DATABASE_URL is set but migrations failed");

        Some(pool)
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let Some(db) = test_db().await else { return };

        // Verify the pool is working
        let one: i32 = sqlx::query_scalar("SELECT 1")
            .fetch_one(&db)
            .await
            .expect("Database query failed");

        assert_eq!(one, 1);
    }

    #[tokio::test]
    async fn test_books_table_exists() {
        let Some(db) = test_db().await else { return };

        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT FROM information_schema.tables WHERE table_name = 'books')",
        )
        .fetch_one(&db)
        .await
        .expect("Query failed");

        assert!(exists);
    }

    #[tokio::test]
    async fn test_collections_crud() {
        let Some(db) = test_db().await else { return };

        let id = uuid::Uuid::now_v7();
        // Create
        sqlx::query("INSERT INTO collections (id, name) VALUES ($1, $2)")
            .bind(id)
            .bind("Test Collection")
            .execute(&db)
            .await
            .expect("Insert failed");

        // Read
        let name: String = sqlx::query_scalar("SELECT name FROM collections WHERE id = $1")
            .bind(id)
            .fetch_one(&db)
            .await
            .expect("Select failed");
        assert_eq!(name, "Test Collection");

        // Delete
        sqlx::query("DELETE FROM collections WHERE id = $1")
            .bind(id)
            .execute(&db)
            .await
            .expect("Delete failed");
    }

    #[tokio::test]
    async fn test_smart_shelves_table() {
        let Some(db) = test_db().await else { return };

        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT FROM information_schema.tables WHERE table_name = 'smart_shelves')",
        )
        .fetch_one(&db)
        .await
        .expect("Query failed");

        assert!(exists);
    }

    #[tokio::test]
    async fn deduplication_schema_supports_explainable_version_evidence() {
        let Some(db) = test_db().await else { return };

        let contracts: Vec<bool> = sqlx::query_scalar(
            r#"SELECT to_regclass(name) IS NOT NULL
               FROM unnest(ARRAY[
                 'book_fingerprints',
                 'chapter_fingerprints',
                 'passage_fingerprints',
                 'dedup_scan_runs',
                 'duplicate_chapter_matches',
                 'book_works',
                 'exact_file_discoveries'
               ]) AS name"#,
        )
        .fetch_all(&db)
        .await
        .expect("deduplication schema query failed");

        assert_eq!(contracts, vec![true; 7]);
    }

    #[tokio::test]
    async fn grouped_dedup_coordinates_reject_partially_null_tuples() {
        let Some(db) = test_db().await else { return };
        let mut tx = db.begin().await.expect("begin grouped constraint test");
        let library_id = uuid::Uuid::now_v7();
        let mut book_ids = [uuid::Uuid::now_v7(), uuid::Uuid::now_v7()];
        book_ids.sort_unstable();
        let pair_id = uuid::Uuid::now_v7();

        sqlx::query("INSERT INTO libraries (id, name, root_path) VALUES ($1, $2, $3)")
            .bind(library_id)
            .bind("Grouped constraint test")
            .bind(format!("/tmp/nova-grouped-constraint-{library_id}"))
            .execute(&mut *tx)
            .await
            .expect("insert grouped constraint library");
        for (offset, book_id) in book_ids.iter().enumerate() {
            sqlx::query(
                r#"INSERT INTO books
                   (id, library_id, title, format, status, file_path, file_hash)
                   VALUES ($1, $2, $3, 'txt', 'ready', $4, $5)"#,
            )
            .bind(book_id)
            .bind(library_id)
            .bind(format!("Grouped constraint book {offset}"))
            .bind(format!("/tmp/{book_id}.txt"))
            .bind(format!("grouped-constraint-hash-{book_id}"))
            .execute(&mut *tx)
            .await
            .expect("insert grouped constraint book");
        }
        sqlx::query(
            r#"INSERT INTO duplicate_pairs
               (id, book_a_id, book_b_id, similarity, method)
               VALUES ($1, $2, $3, 0.9, 'winnowing')"#,
        )
        .bind(pair_id)
        .bind(book_ids[0])
        .bind(book_ids[1])
        .execute(&mut *tx)
        .await
        .expect("insert grouped constraint pair");

        sqlx::query("SAVEPOINT invalid_alignment_tuple")
            .execute(&mut *tx)
            .await
            .expect("create alignment savepoint");
        let alignment_error = sqlx::query(
            r#"INSERT INTO duplicate_chapter_matches
               (pair_id, chapter_a_index, chapter_b_index, match_type,
                alignment_group, segment_ordinal)
               VALUES ($1, 0, 0, 'winnowing', 0, NULL)"#,
        )
        .bind(pair_id)
        .execute(&mut *tx)
        .await
        .expect_err("a partially null alignment tuple must be rejected");
        assert_eq!(
            alignment_error
                .as_database_error()
                .and_then(|error| error.constraint()),
            Some("chk_duplicate_match_alignment_coordinates")
        );
        sqlx::query("ROLLBACK TO SAVEPOINT invalid_alignment_tuple")
            .execute(&mut *tx)
            .await
            .expect("recover from alignment constraint error");

        sqlx::query("SAVEPOINT invalid_range_tuple")
            .execute(&mut *tx)
            .await
            .expect("create range savepoint");
        let range_error = sqlx::query(
            r#"INSERT INTO duplicate_chapter_matches
               (pair_id, chapter_a_index, chapter_b_index, match_type,
                chapter_a_start, chapter_a_end, chapter_b_start, chapter_b_end,
                matched_chars)
               VALUES ($1, 0, 0, 'winnowing', 0, 10, 0, NULL, 10)"#,
        )
        .bind(pair_id)
        .execute(&mut *tx)
        .await
        .expect_err("a partially null text range tuple must be rejected");
        assert_eq!(
            range_error
                .as_database_error()
                .and_then(|error| error.constraint()),
            Some("chk_duplicate_match_text_ranges")
        );
        sqlx::query("ROLLBACK TO SAVEPOINT invalid_range_tuple")
            .execute(&mut *tx)
            .await
            .expect("recover from range constraint error");

        sqlx::query(
            r#"INSERT INTO duplicate_chapter_matches
               (pair_id, chapter_a_index, chapter_b_index, match_type,
                alignment_group, segment_ordinal,
                chapter_a_start, chapter_a_end, chapter_b_start, chapter_b_end,
                matched_chars)
               VALUES ($1, 0, 0, 'winnowing', 0, 0, 0, 10, 0, 10, 10)"#,
        )
        .bind(pair_id)
        .execute(&mut *tx)
        .await
        .expect("a complete grouped match tuple should remain valid");

        tx.rollback()
            .await
            .expect("rollback grouped constraint test");
    }

    #[tokio::test]
    async fn dedup_publication_and_content_invalidation_share_a_book_lock_barrier() {
        let _write_guard = DEDUP_WRITE_TEST_LOCK.lock().await;
        let Some(db) = test_db().await else { return };
        let library_id = uuid::Uuid::now_v7();
        let mut book_ids = [uuid::Uuid::now_v7(), uuid::Uuid::now_v7()];
        book_ids.sort_unstable();
        let chapter_id = uuid::Uuid::now_v7();
        let first_pair_id = uuid::Uuid::now_v7();

        sqlx::query("INSERT INTO libraries (id, name, root_path) VALUES ($1, $2, $3)")
            .bind(library_id)
            .bind("Dedup publication lock test")
            .bind(format!("/tmp/nova-dedup-publication-lock-{library_id}"))
            .execute(&db)
            .await
            .expect("insert publication lock library");
        for (offset, book_id) in book_ids.iter().enumerate() {
            sqlx::query(
                r#"INSERT INTO books
                   (id, library_id, title, format, status, file_path, file_hash)
                   VALUES ($1, $2, $3, 'txt', 'ready', $4, $5)"#,
            )
            .bind(book_id)
            .bind(library_id)
            .bind(format!("Publication lock book {offset}"))
            .bind(format!("/tmp/{book_id}.txt"))
            .bind(format!("publication-lock-hash-{book_id}"))
            .execute(&db)
            .await
            .expect("insert publication lock book");
        }
        sqlx::query(
            r#"INSERT INTO chapters
               (id, book_id, index, chapter_index, title, content)
               VALUES ($1, $2, 0, 0, 'Chapter', 'verified source')"#,
        )
        .bind(chapter_id)
        .bind(book_ids[0])
        .execute(&db)
        .await
        .expect("insert publication lock chapter");
        for book_id in book_ids {
            sqlx::query(
                r#"INSERT INTO book_fingerprints
                   (book_id, normalization_version, algorithm_version,
                    source_content_hash, conservative_hash, layout_hash,
                    chapter_count, informative_chapter_count, char_count,
                    text_integrity_bps)
                   VALUES ($1, 1, 4, $2, $3, $4, 1, 1, 15, 10000)"#,
            )
            .bind(book_id)
            .bind(format!("source-{book_id}"))
            .bind(format!("conservative-{book_id}"))
            .bind(format!("layout-{book_id}"))
            .execute(&db)
            .await
            .expect("insert publication fingerprint");
        }

        // Publication wins the lock first. The content update fails fast with
        // a retryable serialization error, then succeeds after publication and
        // stales the pair instead of forming an advisory/FK wait cycle.
        let mut publisher = db.begin().await.expect("begin publisher transaction");
        sqlx::query("SELECT lock_novel_dedup_books($1)")
            .bind(&book_ids)
            .execute(&mut *publisher)
            .await
            .expect("publisher acquires book locks");
        let update_pool = db.clone();
        let update_attempt = tokio::spawn(async move {
            sqlx::query("UPDATE chapters SET content = 'changed after publish' WHERE id = $1")
                .bind(chapter_id)
                .execute(&update_pool)
                .await
        });
        let update_error = tokio::time::timeout(std::time::Duration::from_secs(1), update_attempt)
            .await
            .expect("content mutation must fail fast instead of deadlocking")
            .expect("join content mutation")
            .expect_err("busy publication lock must reject the first mutation attempt");
        assert_eq!(
            update_error
                .as_database_error()
                .and_then(|error| error.code())
                .as_deref(),
            Some("40001")
        );
        sqlx::query(
            r#"INSERT INTO duplicate_pairs
               (id, book_a_id, book_b_id, similarity, method)
               VALUES ($1, $2, $3, 0.9, 'winnowing')"#,
        )
        .bind(first_pair_id)
        .bind(book_ids[0])
        .bind(book_ids[1])
        .execute(&mut *publisher)
        .await
        .expect("publisher inserts verified pair");
        publisher.commit().await.expect("commit publisher");
        sqlx::query("UPDATE chapters SET content = 'changed after publish' WHERE id = $1")
            .bind(chapter_id)
            .execute(&db)
            .await
            .expect("chapter update retry succeeds after publication");
        let stale: bool = sqlx::query_scalar("SELECT stale FROM duplicate_pairs WHERE id = $1")
            .bind(first_pair_id)
            .fetch_one(&db)
            .await
            .expect("load published pair state");
        assert!(stale, "post-publication content change must stale the pair");

        // Invalidation wins first. Publication yields immediately for durable
        // retry; the retry observes the missing fingerprint snapshot and must
        // not insert a fresh pair.
        sqlx::query("DELETE FROM duplicate_pairs WHERE id = $1")
            .bind(first_pair_id)
            .execute(&db)
            .await
            .expect("clear first publication pair");
        sqlx::query(
            r#"INSERT INTO book_fingerprints
               (book_id, normalization_version, algorithm_version,
                source_content_hash, conservative_hash, layout_hash,
                chapter_count, informative_chapter_count, char_count,
                text_integrity_bps)
               VALUES ($1, 1, 4, $2, $3, $4, 1, 1, 15, 10000)
               ON CONFLICT (book_id) DO UPDATE SET
                 algorithm_version = EXCLUDED.algorithm_version,
                 source_content_hash = EXCLUDED.source_content_hash,
                 conservative_hash = EXCLUDED.conservative_hash,
                 layout_hash = EXCLUDED.layout_hash"#,
        )
        .bind(book_ids[0])
        .bind(format!("source-{}", book_ids[0]))
        .bind(format!("conservative-{}", book_ids[0]))
        .bind(format!("layout-{}", book_ids[0]))
        .execute(&db)
        .await
        .expect("restore invalidated fingerprint");
        let mut invalidator = db.begin().await.expect("begin invalidator transaction");
        sqlx::query("UPDATE chapters SET content = 'invalidation wins' WHERE id = $1")
            .bind(chapter_id)
            .execute(&mut *invalidator)
            .await
            .expect("invalidate fingerprint while retaining book lock");

        let publish_pool = db.clone();
        let publish_book_ids = book_ids;
        let second_pair_id = uuid::Uuid::now_v7();
        let publication_attempt = tokio::spawn(async move {
            let mut tx = publish_pool.begin().await?;
            let acquired: bool = sqlx::query_scalar("SELECT try_lock_novel_dedup_books($1)")
                .bind(&publish_book_ids)
                .fetch_one(&mut *tx)
                .await?;
            tx.rollback().await?;
            Ok::<_, sqlx::Error>(acquired)
        });
        let acquired = tokio::time::timeout(std::time::Duration::from_secs(1), publication_attempt)
            .await
            .expect("publication must fail fast instead of deadlocking")
            .expect("join publication attempt")
            .expect("publication try-lock query succeeds");
        assert!(!acquired, "publication must yield to active invalidation");
        invalidator.commit().await.expect("commit invalidator");

        let mut publication_retry = db.begin().await.expect("begin publication retry");
        let acquired: bool = sqlx::query_scalar("SELECT try_lock_novel_dedup_books($1)")
            .bind(&publish_book_ids)
            .fetch_one(&mut *publication_retry)
            .await
            .expect("publication retry lock query succeeds");
        assert!(acquired, "publication retry should acquire released locks");
        let fingerprint_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM book_fingerprints WHERE book_id = ANY($1)")
                .bind(&publish_book_ids)
                .fetch_one(&mut *publication_retry)
                .await
                .expect("publication retry revalidates fingerprints");
        assert_eq!(fingerprint_count, 1);
        if fingerprint_count == 2 {
            sqlx::query(
                r#"INSERT INTO duplicate_pairs
                   (id, book_a_id, book_b_id, similarity, method)
                   VALUES ($1, $2, $3, 0.9, 'winnowing')"#,
            )
            .bind(second_pair_id)
            .bind(publish_book_ids[0])
            .bind(publish_book_ids[1])
            .execute(&mut *publication_retry)
            .await
            .expect("insert only from a complete current snapshot");
        }
        publication_retry
            .commit()
            .await
            .expect("commit publication retry");
        let inserted: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM duplicate_pairs WHERE id = $1)")
                .bind(second_pair_id)
                .fetch_one(&db)
                .await
                .expect("check rejected publication");
        assert!(!inserted, "stale snapshot must not publish a fresh pair");

        sqlx::query("DELETE FROM books WHERE id = ANY($1)")
            .bind(&book_ids)
            .execute(&db)
            .await
            .expect("clean publication lock books");
        sqlx::query("DELETE FROM libraries WHERE id = $1")
            .bind(library_id)
            .execute(&db)
            .await
            .expect("clean publication lock library");
    }

    #[tokio::test]
    async fn chapter_delete_prelocks_before_fingerprint_cascade_and_publication_retries() {
        let _write_guard = DEDUP_WRITE_TEST_LOCK.lock().await;
        let Some(db) = test_db().await else { return };
        let library_id = uuid::Uuid::now_v7();
        let book_id = uuid::Uuid::now_v7();
        let chapter_id = uuid::Uuid::now_v7();

        sqlx::query("INSERT INTO libraries (id, name, root_path) VALUES ($1, $2, $3)")
            .bind(library_id)
            .bind("Dedup delete prelock test")
            .bind(format!("/tmp/nova-dedup-delete-prelock-{library_id}"))
            .execute(&db)
            .await
            .expect("insert delete prelock library");
        sqlx::query(
            r#"INSERT INTO books
               (id, library_id, title, format, status, file_path, file_hash)
               VALUES ($1, $2, 'Delete prelock book', 'txt', 'ready', $3, $4)"#,
        )
        .bind(book_id)
        .bind(library_id)
        .bind(format!("/tmp/{book_id}.txt"))
        .bind(format!("delete-prelock-hash-{book_id}"))
        .execute(&db)
        .await
        .expect("insert delete prelock book");
        sqlx::query(
            r#"INSERT INTO chapters
               (id, book_id, index, chapter_index, title, content)
               VALUES ($1, $2, 0, 0, 'Chapter', 'delete prelock source')"#,
        )
        .bind(chapter_id)
        .bind(book_id)
        .execute(&db)
        .await
        .expect("insert delete prelock chapter");
        sqlx::query(
            r#"INSERT INTO book_fingerprints
               (book_id, normalization_version, algorithm_version,
                source_content_hash, conservative_hash, layout_hash,
                chapter_count, informative_chapter_count, char_count,
                text_integrity_bps)
               VALUES ($1, 1, 4, $2, $3, $4, 1, 1, 21, 10000)"#,
        )
        .bind(book_id)
        .bind("source-delete-prelock")
        .bind("conservative-delete-prelock")
        .bind("layout-delete-prelock")
        .execute(&db)
        .await
        .expect("insert delete prelock book fingerprint");
        sqlx::query(
            r#"INSERT INTO chapter_fingerprints
               (chapter_id, book_id, chapter_index, normalization_version,
                source_content_hash, conservative_hash, layout_hash,
                char_count, informative, winnowing_count)
               VALUES ($1, $2, 0, 1, $3, $4, $5, 21, TRUE, 1)"#,
        )
        .bind(chapter_id)
        .bind(book_id)
        .bind("source-delete-prelock-chapter")
        .bind("conservative-delete-prelock-chapter")
        .bind("layout-delete-prelock-chapter")
        .execute(&db)
        .await
        .expect("insert delete prelock chapter fingerprint");

        // DELETE must take the advisory lock in BEFORE ROW, before the FK
        // cascade locks the chapter fingerprint. A replacement publication
        // uses the try-lock helper and yields immediately for durable retry.
        let mut deleting = db.begin().await.expect("begin chapter deletion");
        sqlx::query("DELETE FROM chapters WHERE id = $1")
            .bind(chapter_id)
            .execute(&mut *deleting)
            .await
            .expect("delete chapter while retaining prelock");

        let publication_pool = db.clone();
        let publication_attempt = tokio::spawn(async move {
            let mut tx = publication_pool.begin().await?;
            let acquired: bool = sqlx::query_scalar("SELECT try_lock_novel_dedup_books($1)")
                .bind(vec![book_id])
                .fetch_one(&mut *tx)
                .await?;
            tx.rollback().await?;
            Ok::<_, sqlx::Error>(acquired)
        });
        let acquired = tokio::time::timeout(std::time::Duration::from_secs(1), publication_attempt)
            .await
            .expect("try-lock publication must not deadlock behind chapter DELETE")
            .expect("join publication try-lock")
            .expect("publication try-lock query succeeds");
        assert!(!acquired, "busy chapter DELETE must make publication retry");

        deleting.commit().await.expect("commit chapter deletion");

        let mut retry = db.begin().await.expect("begin publication retry");
        let acquired: bool = sqlx::query_scalar("SELECT try_lock_novel_dedup_books($1)")
            .bind(vec![book_id])
            .fetch_one(&mut *retry)
            .await
            .expect("retry publication lock after delete");
        assert!(
            acquired,
            "publication retry should acquire the released lock"
        );
        let chapter_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chapters WHERE book_id = $1")
                .bind(book_id)
                .fetch_one(&mut *retry)
                .await
                .expect("revalidate deleted chapter snapshot");
        let fingerprint_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chapter_fingerprints WHERE book_id = $1")
                .bind(book_id)
                .fetch_one(&mut *retry)
                .await
                .expect("check cascaded chapter fingerprint");
        assert_eq!((chapter_count, fingerprint_count), (0, 0));
        retry.rollback().await.expect("rollback publication retry");

        sqlx::query("DELETE FROM books WHERE id = $1")
            .bind(book_id)
            .execute(&db)
            .await
            .expect("clean delete prelock book");
        sqlx::query("DELETE FROM libraries WHERE id = $1")
            .bind(library_id)
            .execute(&db)
            .await
            .expect("clean delete prelock library");
    }

    #[tokio::test]
    async fn global_dedup_barrier_precedes_cross_book_and_library_cascade_writes() {
        let _write_guard = DEDUP_WRITE_TEST_LOCK.lock().await;
        let Some(db) = test_db().await else { return };
        let library_id = uuid::Uuid::now_v7();
        let book_a = uuid::Uuid::now_v7();
        let book_b = uuid::Uuid::now_v7();
        let chapter_a = uuid::Uuid::now_v7();
        let chapter_b = uuid::Uuid::now_v7();

        sqlx::query("INSERT INTO libraries (id, name, root_path) VALUES ($1, $2, $3)")
            .bind(library_id)
            .bind("Global dedup write barrier test")
            .bind(format!("/tmp/nova-global-dedup-barrier-{library_id}"))
            .execute(&db)
            .await
            .expect("insert global barrier library");
        for (book_id, title) in [(book_a, "Barrier A"), (book_b, "Barrier B")] {
            sqlx::query(
                r#"INSERT INTO books
                   (id, library_id, title, format, status, file_path, file_hash)
                   VALUES ($1, $2, $3, 'txt', 'ready', $4, $5)"#,
            )
            .bind(book_id)
            .bind(library_id)
            .bind(title)
            .bind(format!("/tmp/{book_id}.txt"))
            .bind(format!("global-barrier-hash-{book_id}"))
            .execute(&db)
            .await
            .expect("insert global barrier book");
        }
        for (chapter_id, book_id, content) in [
            (chapter_a, book_a, "barrier source a"),
            (chapter_b, book_b, "barrier source b"),
        ] {
            sqlx::query(
                r#"INSERT INTO chapters
                   (id, book_id, index, chapter_index, title, content)
                   VALUES ($1, $2, 0, 0, 'Chapter', $3)"#,
            )
            .bind(chapter_id)
            .bind(book_id)
            .bind(content)
            .execute(&db)
            .await
            .expect("insert global barrier chapter");
        }

        // The first content statement takes the transaction-wide barrier. A
        // second transaction fails before touching B's tuple, so transaction 1
        // can safely update B without a chapter/pair/library wait cycle.
        let mut first = db.begin().await.expect("begin first barrier writer");
        sqlx::query("UPDATE chapters SET content = 'first writes a' WHERE id = $1")
            .bind(chapter_a)
            .execute(&mut *first)
            .await
            .expect("first writer acquires global barrier");

        let second_pool = db.clone();
        let second_attempt = tokio::spawn(async move {
            sqlx::query("UPDATE chapters SET content = 'second writes b' WHERE id = $1")
                .bind(chapter_b)
                .execute(&second_pool)
                .await
        });
        let second_error = tokio::time::timeout(std::time::Duration::from_secs(1), second_attempt)
            .await
            .expect("second writer must fail fast before a deadlock")
            .expect("join second writer")
            .expect_err("busy global barrier rejects concurrent content mutation");
        assert_eq!(
            second_error
                .as_database_error()
                .and_then(|error| error.code())
                .as_deref(),
            Some("40001")
        );
        sqlx::query("UPDATE chapters SET content = 'first also writes b' WHERE id = $1")
            .bind(chapter_b)
            .execute(&mut *first)
            .await
            .expect("global barrier owner can update the second book");

        let delete_pool = db.clone();
        let delete_attempt = tokio::spawn(async move {
            sqlx::query("DELETE FROM libraries WHERE id = $1")
                .bind(library_id)
                .execute(&delete_pool)
                .await
        });
        let delete_error = tokio::time::timeout(std::time::Duration::from_secs(1), delete_attempt)
            .await
            .expect("library detach must fail fast before touching child tuples")
            .expect("join library delete")
            .expect_err("busy global barrier rejects library detach");
        assert_eq!(
            delete_error
                .as_database_error()
                .and_then(|error| error.code())
                .as_deref(),
            Some("40001")
        );

        first.commit().await.expect("commit first barrier writer");
        let scan_task_ids: Vec<uuid::Uuid> = sqlx::query_scalar(
            "SELECT task_id FROM dedup_scan_runs WHERE library_id = $1 AND task_id IS NOT NULL",
        )
        .bind(library_id)
        .fetch_all(&db)
        .await
        .expect("capture triggered scan tasks for cleanup");
        sqlx::query("DELETE FROM libraries WHERE id = $1")
            .bind(library_id)
            .execute(&db)
            .await
            .expect("library detach retry succeeds after barrier release");
        sqlx::query("DELETE FROM books WHERE id = ANY($1)")
            .bind(vec![book_a, book_b])
            .execute(&db)
            .await
            .expect("clean detached global barrier books");
        if !scan_task_ids.is_empty() {
            sqlx::query("DELETE FROM tasks WHERE id = ANY($1)")
                .bind(scan_task_ids)
                .execute(&db)
                .await
                .expect("clean global barrier scan tasks");
        }
    }

    #[tokio::test]
    async fn durable_task_status_atomically_projects_to_dedup_scan_without_payload_decode() {
        let Some(db) = test_db().await else { return };
        let mut tx = db.begin().await.expect("begin scan projection test");
        let task_id = uuid::Uuid::now_v7();
        let scan_id = uuid::Uuid::now_v7();

        sqlx::query(
            r#"INSERT INTO tasks
               (id, kind, status, priority, payload, category,
                retry_count, max_retries, scheduled_at, started_at)
               VALUES ($1, 'deduplicate', 'running', '1',
                       '{"malformed_scan_payload":true}'::jsonb,
                       'preprocess', 3, 3, NOW(), NOW())"#,
        )
        .bind(task_id)
        .execute(&mut *tx)
        .await
        .expect("insert malformed durable scan task");
        sqlx::query(
            r#"INSERT INTO dedup_scan_runs
               (id, task_id, include_semantic, algorithm_version, status,
                progress, progress_message)
               VALUES ($1, $2, FALSE, 4, 'running', 73, 'verifying')"#,
        )
        .bind(scan_id)
        .bind(task_id)
        .execute(&mut *tx)
        .await
        .expect("insert running scan projection");

        sqlx::query(
            r#"UPDATE tasks
               SET status = 'dead_letter', error_message = 'terminal failure',
                   completed_at = NOW()
               WHERE id = $1"#,
        )
        .bind(task_id)
        .execute(&mut *tx)
        .await
        .expect("dead-letter task and project scan in one statement");

        let state: (String, Option<String>, Option<String>, bool) = sqlx::query_as(
            r#"SELECT status, progress_message, error_message,
                      completed_at IS NOT NULL
               FROM dedup_scan_runs WHERE id = $1"#,
        )
        .bind(scan_id)
        .fetch_one(&mut *tx)
        .await
        .expect("load atomically projected scan state");
        assert_eq!(state.0, "failed");
        assert_eq!(state.1.as_deref(), Some("failed"));
        assert_eq!(state.2.as_deref(), Some("terminal failure"));
        assert!(state.3);

        tx.rollback().await.expect("rollback scan projection test");
    }

    #[tokio::test]
    async fn failing_index_maintenance_cannot_downgrade_a_completed_scan_projection() {
        let Some(db) = test_db().await else { return };
        let mut tx = db.begin().await.expect("begin maintenance projection test");
        let task_id = uuid::Uuid::now_v7();
        let scan_id = uuid::Uuid::now_v7();

        sqlx::query(
            r#"INSERT INTO tasks
               (id, kind, status, priority, payload, category,
                retry_count, max_retries, scheduled_at)
               VALUES ($1, 'deduplicate', 'running', '1',
                       '{"operation":"cleanup_secondary_indexes",
                         "secondary_book_id":"not-a-uuid",
                         "primary_book_id":"not-a-uuid"}'::jsonb,
                       'maintenance', 0, 0, NOW())"#,
        )
        .bind(task_id)
        .execute(&mut *tx)
        .await
        .expect("insert failing index maintenance task");
        sqlx::query(
            r#"INSERT INTO dedup_scan_runs
               (id, task_id, include_semantic, algorithm_version, status,
                progress, progress_message, completed_at)
               VALUES ($1, $2, FALSE, 4, 'completed', 100, 'completed', NOW())"#,
        )
        .bind(scan_id)
        .bind(task_id)
        .execute(&mut *tx)
        .await
        .expect("insert completed scan linked to maintenance task");

        sqlx::query(
            r#"UPDATE tasks
               SET status = 'dead_letter', error_message = 'maintenance failure',
                   completed_at = NOW()
               WHERE id = $1"#,
        )
        .bind(task_id)
        .execute(&mut *tx)
        .await
        .expect("dead-letter index maintenance task");

        let state: (String, i16, Option<String>, Option<String>) = sqlx::query_as(
            r#"SELECT status, progress, progress_message, error_message
               FROM dedup_scan_runs WHERE id = $1"#,
        )
        .bind(scan_id)
        .fetch_one(&mut *tx)
        .await
        .expect("load completed scan after maintenance failure");
        assert_eq!(state.0, "completed");
        assert_eq!(state.1, 100);
        assert_eq!(state.2.as_deref(), Some("completed"));
        assert_eq!(state.3, None);

        tx.rollback()
            .await
            .expect("rollback maintenance projection test");
    }

    #[tokio::test]
    async fn resolution_barrier_excludes_work_parent_cascades_before_member_rows() {
        let _write_guard = DEDUP_WRITE_TEST_LOCK.lock().await;
        let Some(db) = test_db().await else { return };
        let library_id = uuid::Uuid::now_v7();
        let work_id = uuid::Uuid::now_v7();
        let book_ids = [uuid::Uuid::now_v7(), uuid::Uuid::now_v7()];

        sqlx::query("INSERT INTO libraries (id, name, root_path) VALUES ($1, $2, $3)")
            .bind(library_id)
            .bind("Resolution work barrier test")
            .bind(format!("/tmp/nova-resolution-work-{library_id}"))
            .execute(&db)
            .await
            .expect("insert resolution work library");
        for (offset, book_id) in book_ids.iter().enumerate() {
            sqlx::query(
                r#"INSERT INTO books
                   (id, library_id, title, format, status, file_path, file_hash)
                   VALUES ($1, $2, $3, 'txt', 'ready', $4, $5)"#,
            )
            .bind(book_id)
            .bind(library_id)
            .bind(format!("Resolution work member {offset}"))
            .bind(format!("/tmp/{book_id}.txt"))
            .bind(format!("resolution-work-hash-{book_id}"))
            .execute(&db)
            .await
            .expect("insert resolution work member");
        }
        sqlx::query(
            r#"INSERT INTO book_works (id, canonical_title, primary_book_id)
               VALUES ($1, 'Resolution Work', $2)"#,
        )
        .bind(work_id)
        .bind(book_ids[0])
        .execute(&db)
        .await
        .expect("insert resolution work");
        sqlx::query("UPDATE books SET work_id = $1 WHERE id = ANY($2)")
            .bind(work_id)
            .bind(book_ids.as_slice())
            .execute(&db)
            .await
            .expect("attach resolution work members");

        let mut resolution = db.begin().await.expect("begin simulated resolution");
        let acquired: bool = sqlx::query_scalar("SELECT try_lock_novel_dedup_global_barrier()")
            .fetch_one(&mut *resolution)
            .await
            .expect("resolution acquires global barrier before pair/work rows");
        assert!(acquired);
        sqlx::query("SELECT lock_novel_dedup_books($1)")
            .bind(book_ids.as_slice())
            .execute(&mut *resolution)
            .await
            .expect("resolution locks complete work membership");

        let delete_pool = db.clone();
        let delete_attempt = tokio::spawn(async move {
            sqlx::query("DELETE FROM book_works WHERE id = $1")
                .bind(work_id)
                .execute(&delete_pool)
                .await
        });
        let delete_error = tokio::time::timeout(std::time::Duration::from_secs(1), delete_attempt)
            .await
            .expect("work parent delete must fail before member-row cascade")
            .expect("join work parent delete")
            .expect_err("resolution barrier rejects concurrent work delete");
        assert_eq!(
            delete_error
                .as_database_error()
                .and_then(|error| error.code())
                .as_deref(),
            Some("40001")
        );

        resolution
            .rollback()
            .await
            .expect("release resolution barrier");
        sqlx::query("DELETE FROM book_works WHERE id = $1")
            .bind(work_id)
            .execute(&db)
            .await
            .expect("work delete retry succeeds after resolution rollback");
        sqlx::query("DELETE FROM books WHERE id = ANY($1)")
            .bind(book_ids.as_slice())
            .execute(&db)
            .await
            .expect("clean resolution work books");
        sqlx::query(
            r#"DELETE FROM tasks
               WHERE id IN (
                 SELECT task_id FROM dedup_scan_runs
                 WHERE library_id = $1 AND task_id IS NOT NULL
               )"#,
        )
        .bind(library_id)
        .execute(&db)
        .await
        .expect("clean resolution work scan tasks");
        sqlx::query("DELETE FROM libraries WHERE id = $1")
            .bind(library_id)
            .execute(&db)
            .await
            .expect("clean resolution work library");
    }

    #[tokio::test]
    async fn imported_books_and_real_chapter_changes_coalesce_targeted_scans() {
        let _write_guard = DEDUP_WRITE_TEST_LOCK.lock().await;
        let Some(db) = test_db().await else { return };
        let mut tx = db.begin().await.expect("begin dedup trigger test");
        let library_id = uuid::Uuid::now_v7();
        let book_a = uuid::Uuid::now_v7();
        let book_b = uuid::Uuid::now_v7();

        sqlx::query("INSERT INTO libraries (id, name, root_path) VALUES ($1, $2, $3)")
            .bind(library_id)
            .bind("Incremental dedup trigger test")
            .bind(format!("/tmp/nova-incremental-dedup-{library_id}"))
            .execute(&mut *tx)
            .await
            .expect("insert trigger test library");

        for (book_id, title) in [(book_a, "Trigger A"), (book_b, "Trigger B")] {
            sqlx::query(
                r#"INSERT INTO books
                   (id, library_id, title, format, status, file_path, file_hash)
                   VALUES ($1, $2, $3, 'txt', 'ready', $4, $5)"#,
            )
            .bind(book_id)
            .bind(library_id)
            .bind(title)
            .bind(format!("/tmp/{book_id}.txt"))
            .bind(format!("hash-{book_id}"))
            .execute(&mut *tx)
            .await
            .expect("insert trigger test book");
        }

        let queued = sqlx::query_as::<_, QueuedAutomaticScan>(
            r#"SELECT task.id AS task_id, task.payload, task.scheduled_at,
                      scan.books_total, scan.requested_by,
                      task.book_id AS task_book_id
               FROM dedup_scan_runs scan
               JOIN tasks task ON task.id = scan.task_id
               WHERE scan.library_id = $1
                 AND scan.status = 'queued'
                 AND task.status = 'queued'"#,
        )
        .bind(library_id)
        .fetch_all(&mut *tx)
        .await
        .expect("load coalesced automatic scan");

        assert_eq!(
            queued.len(),
            1,
            "one library must have one queued follow-up"
        );
        let queued = &queued[0];
        assert_eq!(queued.payload["operation"], "scan");
        assert_eq!(queued.books_total, 2);
        assert_eq!(
            queued.requested_by, None,
            "triggered scans are system requested"
        );
        assert_eq!(
            queued.task_book_id, None,
            "a coalesced scan must not be deleted with its first target book"
        );
        let mut targets: Vec<uuid::Uuid> = queued.payload["target_book_ids"]
            .as_array()
            .expect("automatic scan target list")
            .iter()
            .map(|value| {
                uuid::Uuid::parse_str(value.as_str().expect("UUID target string"))
                    .expect("valid UUID target")
            })
            .collect();
        targets.sort_unstable();
        let mut expected_targets = vec![book_a, book_b];
        expected_targets.sort_unstable();
        assert_eq!(targets, expected_targets);

        let task_locks: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM task_execution_locks WHERE task_id = $1")
                .bind(queued.task_id)
                .fetch_one(&mut *tx)
                .await
                .expect("count automatic scan resources");
        assert_eq!(task_locks, 2);

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let chapter_id = uuid::Uuid::now_v7();
        sqlx::query(
            r#"INSERT INTO chapters
               (id, book_id, index, chapter_index, title, content)
               VALUES ($1, $2, 0, 0, 'Chapter', 'original content')"#,
        )
        .bind(chapter_id)
        .bind(book_a)
        .execute(&mut *tx)
        .await
        .expect("insert changed chapter");
        let changed_schedule: chrono::DateTime<chrono::Utc> =
            sqlx::query_scalar("SELECT scheduled_at FROM tasks WHERE id = $1")
                .bind(queued.task_id)
                .fetch_one(&mut *tx)
                .await
                .expect("load schedule after chapter insert");
        assert!(changed_schedule > queued.scheduled_at);

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        sqlx::query("UPDATE chapters SET content = content WHERE id = $1")
            .bind(chapter_id)
            .execute(&mut *tx)
            .await
            .expect("write identical chapter content");
        let identical_schedule: chrono::DateTime<chrono::Utc> =
            sqlx::query_scalar("SELECT scheduled_at FROM tasks WHERE id = $1")
                .bind(queued.task_id)
                .fetch_one(&mut *tx)
                .await
                .expect("load schedule after identical update");
        assert_eq!(identical_schedule, changed_schedule);

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        sqlx::query("UPDATE chapters SET content = content || ' changed' WHERE id = $1")
            .bind(chapter_id)
            .execute(&mut *tx)
            .await
            .expect("write changed chapter content");
        let rescheduled: chrono::DateTime<chrono::Utc> =
            sqlx::query_scalar("SELECT scheduled_at FROM tasks WHERE id = $1")
                .bind(queued.task_id)
                .fetch_one(&mut *tx)
                .await
                .expect("load schedule after real update");
        assert!(rescheduled > identical_schedule);

        sqlx::query("DELETE FROM books WHERE id = $1")
            .bind(book_a)
            .execute(&mut *tx)
            .await
            .expect("delete first automatic scan target");
        let surviving_payload: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT payload FROM tasks WHERE id = $1")
                .bind(queued.task_id)
                .fetch_optional(&mut *tx)
                .await
                .expect("load automatic scan after deleting first target");
        let surviving_payload =
            surviving_payload.expect("coalesced scan task must survive first target deletion");
        assert!(surviving_payload["target_book_ids"]
            .as_array()
            .is_some_and(|targets| targets.iter().any(|target| target == &book_b.to_string())));

        tx.rollback().await.expect("rollback dedup trigger test");
    }
}
