//! End-to-end verification for content containment deduplication.
//!
//! This test intentionally runs the real `nova-api` binary and worker against a
//! disposable PostgreSQL database. Run it explicitly with:
//!
//! ```text
//! DATABASE_URL=postgres://... cargo test -p nova-api --test dedup_e2e -- --ignored --nocapture
//! ```

use std::net::TcpListener;
use std::path::Path;
use std::process::Stdio;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use nova_ingest::dedup::DEDUP_ALGORITHM_VERSION;
use reqwest::{Client, Response};
use serde_json::{json, Value};
use sqlx::postgres::PgConnectOptions;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use tokio::process::{Child, Command};
use uuid::Uuid;

const SHORT_BOOK_ID: &str = "00000000-0000-0000-0000-000000000001";
const LONG_BOOK_ID: &str = "00000000-0000-0000-0000-000000000002";
const APPROXIMATE_ANNOTATION_ID: &str = "10000000-0000-0000-0000-000000000001";
const RELIABLE_ANNOTATION_ID: &str = "10000000-0000-0000-0000-000000000002";
const APPROXIMATE_BOOKMARK_ID: &str = "10000000-0000-0000-0000-000000000003";
const APPROXIMATE_PROGRESS_ID: &str = "10000000-0000-0000-0000-000000000004";
const PROGRESS_CONFLICT_USER_ID: &str = "10000000-0000-0000-0000-000000000005";
const SHORT_COMPLETE_PROGRESS_ID: &str = "10000000-0000-0000-0000-000000000006";
const LONG_LATER_PROGRESS_ID: &str = "10000000-0000-0000-0000-000000000007";
const PROVENANCE_COLLECTION_ID: &str = "20000000-0000-0000-0000-000000000001";
const PROVENANCE_SHELF_ID: &str = "20000000-0000-0000-0000-000000000002";
const PROVENANCE_TAG_ID: &str = "20000000-0000-0000-0000-000000000003";
const PROVENANCE_SERIES_ID: &str = "20000000-0000-0000-0000-000000000004";
const PROVENANCE_TRANSLATOR_ID: &str = "20000000-0000-0000-0000-000000000005";
const PROVENANCE_EDITOR_ID: &str = "20000000-0000-0000-0000-000000000006";
const APPROXIMATE_ANNOTATION_TEXT: &str = "近似章节批注必须保留在短版";
const RELIABLE_ANNOTATION_TEXT: &str = "严格相同章节批注应迁移到长版";
const APPROXIMATE_BOOKMARK_TITLE: &str = "近似章节书签必须保留在短版";
static TEST_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[tokio::test]
#[ignore = "requires local PostgreSQL and Redis; creates and drops an isolated database"]
async fn shorter_novel_is_detected_as_a_contained_version_through_the_http_api() {
    let base_url = std::env::var("DATABASE_URL")
        .context(
            "DATABASE_URL must point to a PostgreSQL server where the user can create databases",
        )
        .expect("dedup e2e prerequisites");
    let database_name = format!("nova_dedup_e2e_{}", Uuid::now_v7().simple());
    let database_url = database_url_for(&base_url, &database_name)
        .expect("DATABASE_URL must use postgres:// or postgresql:// URL syntax");

    create_database(&base_url, &database_name)
        .await
        .expect("create isolated dedup e2e database");
    if let Err(error) = prepare_isolated_schema(&database_url).await {
        let _ = drop_database(&base_url, &database_name).await;
        panic!("prepare isolated dedup e2e schema: {error:#}");
    }

    let temp_root = std::env::temp_dir().join(format!("nova-dedup-e2e-{}", Uuid::now_v7()));
    let port = reserve_port().expect("reserve nova-api e2e port");
    let mut server = match spawn_server(&database_url, port, &temp_root) {
        Ok(server) => server,
        Err(error) => {
            let _ = drop_database(&base_url, &database_name).await;
            panic!("spawn nova-api for dedup e2e: {error:#}");
        }
    };

    let scenario_result = run_scenario(port, &database_url, &temp_root, &mut server).await;
    stop_server(&mut server).await;
    let cleanup_result = drop_database(&base_url, &database_name).await;
    let _ = tokio::fs::remove_dir_all(&temp_root).await;

    if let Err(error) = cleanup_result {
        panic!("drop isolated dedup e2e database: {error:#}");
    }
    if let Err(error) = scenario_result {
        panic!("dedup containment e2e failed: {error:#}");
    }
}

#[tokio::test]
#[ignore = "requires local PostgreSQL and Redis; creates and drops an isolated database"]
async fn merged_chapter_boundaries_round_trip_through_http_and_worker() {
    let base_url = std::env::var("DATABASE_URL")
        .context(
            "DATABASE_URL must point to a PostgreSQL server where the user can create databases",
        )
        .expect("grouped dedup e2e prerequisites");
    let database_name = format!("nova_dedup_grouped_e2e_{}", Uuid::now_v7().simple());
    let database_url = database_url_for(&base_url, &database_name)
        .expect("DATABASE_URL must use postgres:// or postgresql:// URL syntax");

    create_database(&base_url, &database_name)
        .await
        .expect("create isolated grouped dedup e2e database");
    if let Err(error) = prepare_isolated_schema(&database_url).await {
        let _ = drop_database(&base_url, &database_name).await;
        panic!("prepare isolated grouped dedup e2e schema: {error:#}");
    }

    let temp_root = std::env::temp_dir().join(format!("nova-dedup-grouped-e2e-{}", Uuid::now_v7()));
    let port = reserve_port().expect("reserve grouped nova-api e2e port");
    let mut server = match spawn_server(&database_url, port, &temp_root) {
        Ok(server) => server,
        Err(error) => {
            let _ = drop_database(&base_url, &database_name).await;
            panic!("spawn nova-api for grouped dedup e2e: {error:#}");
        }
    };

    let scenario_result =
        run_grouped_boundary_scenario(port, &database_url, &temp_root, &mut server).await;
    stop_server(&mut server).await;
    let cleanup_result = drop_database(&base_url, &database_name).await;
    let _ = tokio::fs::remove_dir_all(&temp_root).await;

    if let Err(error) = cleanup_result {
        panic!("drop isolated grouped dedup e2e database: {error:#}");
    }
    if let Err(error) = scenario_result {
        panic!("grouped dedup e2e failed: {error:#}");
    }
}

async fn run_grouped_boundary_scenario(
    port: u16,
    database_url: &str,
    temp_root: &Path,
    server: &mut Child,
) -> Result<()> {
    let origin = format!("http://127.0.0.1:{port}");
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("build grouped e2e HTTP client")?;
    wait_until_ready(&client, &origin, server).await?;

    let register = client
        .post(format!("{origin}/api/auth/register"))
        .json(&json!({
            "username": "dedup-grouped-e2e-admin",
            "password": "DedupGroupedE2ePassword!",
            "display_name": "Dedup Grouped E2E"
        }))
        .send()
        .await
        .context("register grouped isolated admin through API")?;
    let register_data = response_data(register).await?;
    let access_token = register_data
        .get("access_token")
        .and_then(Value::as_str)
        .context("grouped register response missing access_token")?;
    let user_id = register_data
        .get("user")
        .and_then(|user| user.get("id"))
        .and_then(Value::as_str)
        .context("grouped register response missing user.id")
        .and_then(|value| Uuid::parse_str(value).context("grouped user.id is not a UUID"))?;

    let pool = PgPool::connect(database_url)
        .await
        .context("connect grouped fixture writer to isolated database")?;
    let library_root = temp_root.join("watched-library");
    let library_id = seed_grouped_boundary_fixture(&pool, &library_root).await?;

    let scan = client
        .post(format!("{origin}/api/duplicates/scans"))
        .bearer_auth(access_token)
        .json(&json!({
            "library_id": library_id,
            "include_semantic": false
        }))
        .send()
        .await
        .context("start grouped dedup scan through API")?;
    let scan = response_data(scan).await?;
    let scan_id = scan
        .get("id")
        .and_then(Value::as_str)
        .context("grouped scan response missing id")?;
    let completed = wait_for_completed_scan(&client, &origin, access_token, scan_id).await?;
    expect_i64(
        &completed,
        "algorithm_version",
        i64::from(DEDUP_ALGORITHM_VERSION),
    )?;
    expect_i64(&completed, "books_processed", 2)?;
    expect_i64(&completed, "chapters_processed", 16)?;
    expect_i64(&completed, "pairs_found", 1)?;
    expect_i64(&completed, "contained_pairs", 1)?;

    let pairs = client
        .get(format!(
            "{origin}/api/duplicates?library_id={library_id}&relation=contained_version"
        ))
        .bearer_auth(access_token)
        .send()
        .await
        .context("list grouped contained pair through API")?;
    let pairs = response_data(pairs).await?;
    expect_i64(&pairs, "total", 1)?;
    let pair = pairs
        .get("items")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .context("grouped duplicates response missing pair")?;
    expect_str(pair, "relation", "contained_version")?;
    expect_str(pair, "contained_book_id", SHORT_BOOK_ID)?;
    expect_str(pair, "recommended_primary_id", LONG_BOOK_ID)?;
    expect_i64(pair, "shared_chapters", 10)?;
    expect_approx(pair, "coverage_a", 1.0, 0.000_001)?;
    expect_approx(pair, "coverage_b", 1.0 / 6.0, 0.000_001)?;
    expect_approx(pair, "character_coverage_a", 1.0, 0.000_001)?;
    let character_coverage_b = pair
        .get("character_coverage_b")
        .and_then(Value::as_f64)
        .context("grouped pair missing character_coverage_b")?;
    if !(0.60..0.80).contains(&character_coverage_b) {
        bail!(
            "merged long version should retain substantial unique text; got character_coverage_b={character_coverage_b}"
        );
    }
    expect_approx(pair, "order_score", 1.0, 0.000_001)?;
    let pair_id = pair
        .get("id")
        .and_then(Value::as_str)
        .context("grouped pair missing id")?;

    let detail = client
        .get(format!("{origin}/api/duplicates/{pair_id}"))
        .bearer_auth(access_token)
        .send()
        .await
        .context("load grouped pair detail through API")?;
    let detail = response_data(detail).await?;
    let groups = detail
        .pointer("/evidence/chapter_boundary_groups")
        .and_then(Value::as_array)
        .context("grouped pair evidence missing chapter_boundary_groups")?;
    if groups.len() != 1 {
        bail!("ten source chapters merged into one target chapter must form one alignment group; got {groups:?}");
    }
    let group = &groups[0];
    expect_str(group, "mapping_shape", "many_to_one")?;
    expect_i64(group, "segment_count", 10)?;
    let group_id = group
        .get("id")
        .and_then(Value::as_i64)
        .context("grouped alignment missing id")?;

    let matches = detail
        .get("chapter_matches")
        .and_then(Value::as_array)
        .context("grouped pair detail missing chapter_matches")?;
    expect_i64(&detail, "chapter_matches_total", 10)?;
    if matches.len() != 10 {
        bail!(
            "ten-to-one alignment must persist ten fragments; got {}",
            matches.len()
        );
    }
    let mut previous_target_end = 0_i64;
    for (ordinal, chapter_match) in matches.iter().enumerate() {
        expect_str(chapter_match, "match_type", "winnowing")?;
        expect_i64(chapter_match, "alignment_group", group_id)?;
        expect_i64(chapter_match, "segment_ordinal", ordinal as i64)?;
        expect_i64(chapter_match, "chapter_a_index", ordinal as i64)?;
        expect_i64(chapter_match, "chapter_b_index", 0)?;
        let source_start = chapter_match
            .get("chapter_a_start")
            .and_then(Value::as_i64)
            .context("grouped fragment missing chapter_a_start")?;
        let source_end = chapter_match
            .get("chapter_a_end")
            .and_then(Value::as_i64)
            .context("grouped fragment missing chapter_a_end")?;
        let target_start = chapter_match
            .get("chapter_b_start")
            .and_then(Value::as_i64)
            .context("grouped fragment missing chapter_b_start")?;
        let target_end = chapter_match
            .get("chapter_b_end")
            .and_then(Value::as_i64)
            .context("grouped fragment missing chapter_b_end")?;
        let matched_chars = chapter_match
            .get("matched_chars")
            .and_then(Value::as_i64)
            .context("grouped fragment missing matched_chars")?;
        if source_start != 0
            || source_end <= source_start
            || target_start != previous_target_end
            || target_end <= target_start
            || matched_chars != source_end - source_start
            || matched_chars != target_end - target_start
        {
            bail!("invalid grouped fragment round-trip at ordinal {ordinal}: {chapter_match}");
        }
        previous_target_end = target_end;
    }

    let first_match_id = matches[0]
        .get("id")
        .and_then(Value::as_str)
        .context("first grouped fragment missing id")?;
    let grouped_source_chapter_id = matches[0]
        .get("chapter_a_id")
        .and_then(Value::as_str)
        .context("first grouped fragment missing source chapter id")
        .and_then(|value| {
            Uuid::parse_str(value).context("first grouped source chapter id is invalid")
        })?;
    seed_grouped_reader_assets(&pool, user_id, grouped_source_chapter_id).await?;
    let diff = client
        .get(format!(
            "{origin}/api/duplicates/{pair_id}/matches/{first_match_id}/diff"
        ))
        .bearer_auth(access_token)
        .send()
        .await
        .context("load grouped segment diff through API")?;
    let diff = response_data(diff).await?;
    expect_approx(&diff, "ratio", 1.0, 0.000_001)?;
    if diff
        .get("changes")
        .and_then(Value::as_array)
        .is_none_or(|changes| {
            changes.is_empty()
                || changes
                    .iter()
                    .any(|change| change.get("tag").and_then(Value::as_str) != Some("equal"))
        })
    {
        bail!("grouped diff must compare only the source-verified segment: {diff}");
    }

    let resolution = client
        .post(format!("{origin}/api/duplicates/{pair_id}/resolve"))
        .bearer_auth(access_token)
        .json(&json!({ "action": "keep_b" }))
        .send()
        .await
        .context("resolve grouped contained version through API")?;
    let resolution = response_data(resolution).await?;
    expect_str(&resolution, "status", "confirmed")?;
    let cleanup_task_id = resolution
        .get("index_cleanup_task_id")
        .and_then(Value::as_str)
        .context("grouped resolution missing cleanup task id")
        .and_then(|value| Uuid::parse_str(value).context("grouped cleanup task id is invalid"))?;
    wait_for_task_status(&pool, cleanup_task_id, "completed").await?;

    let redundant_policy: Vec<bool> = sqlx::query_scalar(
        r#"SELECT dedup_chapter_is_redundant(id)
           FROM chapters
           WHERE book_id = $1
           ORDER BY chapter_index"#,
    )
    .bind(Uuid::parse_str(SHORT_BOOK_ID)?)
    .fetch_all(&pool)
    .await
    .context("load grouped search redundancy policy")?;
    if redundant_policy.len() != 10 || redundant_policy.iter().any(|redundant| *redundant) {
        bail!(
            "grouped winnowing fragments must not hide chapters from search: {redundant_policy:?}"
        );
    }
    let cleanup_result: Option<Value> =
        sqlx::query_scalar("SELECT result FROM tasks WHERE id = $1")
            .bind(cleanup_task_id)
            .fetch_one(&pool)
            .await
            .context("load grouped cleanup task result")?;
    let redundant_indexes = cleanup_result
        .as_ref()
        .and_then(|result| result.get("redundant_chapter_indexes"))
        .and_then(Value::as_array)
        .context("grouped cleanup task must report redundant chapter indexes")?;
    if !redundant_indexes.is_empty() {
        bail!("grouped fragments must not enter strict index cleanup: {redundant_indexes:?}");
    }
    assert_grouped_reader_assets_stayed_on_source(&pool, grouped_source_chapter_id).await?;

    pool.close().await;
    Ok(())
}

async fn run_scenario(
    port: u16,
    database_url: &str,
    temp_root: &Path,
    server: &mut Child,
) -> Result<()> {
    let origin = format!("http://127.0.0.1:{port}");
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("build e2e HTTP client")?;
    wait_until_ready(&client, &origin, server).await?;

    let register = client
        .post(format!("{origin}/api/auth/register"))
        .json(&json!({
            "username": "dedup-e2e-admin",
            "password": "DedupE2ePassword!",
            "display_name": "Dedup E2E"
        }))
        .send()
        .await
        .context("register isolated admin through API")?;
    let register_data = response_data(register).await?;
    let access_token = register_data
        .get("access_token")
        .and_then(Value::as_str)
        .context("register response missing access_token")?;
    let user_id = register_data
        .get("user")
        .and_then(|user| user.get("id"))
        .and_then(Value::as_str)
        .context("register response missing user.id")
        .and_then(|value| Uuid::parse_str(value).context("register user.id is not a UUID"))?;

    let pool = PgPool::connect(database_url)
        .await
        .context("connect fixture writer to isolated database")?;
    let library_root = temp_root.join("watched-library");
    let library_id = seed_containment_fixture(&pool, &library_root).await?;

    let scan = client
        .post(format!("{origin}/api/duplicates/scans"))
        .bearer_auth(access_token)
        .json(&json!({
            "library_id": library_id,
            "include_semantic": false
        }))
        .send()
        .await
        .context("start dedup scan through API")?;
    let scan_data = response_data(scan).await?;
    let scan_id = scan_data
        .get("id")
        .and_then(Value::as_str)
        .context("scan response missing id")?;

    let completed_scan = wait_for_completed_scan(&client, &origin, access_token, scan_id).await?;
    expect_str(&completed_scan, "progress_message", "completed")?;
    expect_i64(
        &completed_scan,
        "algorithm_version",
        i64::from(DEDUP_ALGORITHM_VERSION),
    )?;
    expect_i64(&completed_scan, "books_processed", 2)?;
    expect_i64(&completed_scan, "chapters_processed", 25)?;
    expect_i64(&completed_scan, "pairs_found", 1)?;
    expect_i64(&completed_scan, "contained_pairs", 1)?;
    let task_resources: Vec<(String, String)> = sqlx::query_as(
        r#"SELECT resource_key, mode
           FROM task_execution_locks resource
           JOIN dedup_scan_runs scan ON scan.task_id = resource.task_id
           WHERE scan.id = $1
           ORDER BY resource_key"#,
    )
    .bind(Uuid::parse_str(scan_id)?)
    .fetch_all(&pool)
    .await
    .context("load persistent scan task resources")?;
    let expected_resources = vec![
        ("dedup:scan:barrier".to_string(), "shared".to_string()),
        (
            format!("dedup:scan:library:{library_id}"),
            "exclusive".to_string(),
        ),
    ];
    if task_resources != expected_resources {
        bail!("library scans must declare generic shared/exclusive task resources; got {task_resources:?}");
    }

    let fingerprint_times_before: Vec<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT computed_at FROM book_fingerprints ORDER BY book_id")
            .fetch_all(&pool)
            .await
            .context("load fingerprint timestamps before incremental rescan")?;
    let second_scan = client
        .post(format!("{origin}/api/duplicates/scans"))
        .bearer_auth(access_token)
        .json(&json!({
            "library_id": library_id,
            "include_semantic": false
        }))
        .send()
        .await
        .context("start incremental dedup rescan through API")?;
    let second_scan_data = response_data(second_scan).await?;
    let second_scan_id = second_scan_data
        .get("id")
        .and_then(Value::as_str)
        .context("incremental scan response missing id")?;
    let second_completed_scan =
        wait_for_completed_scan(&client, &origin, access_token, second_scan_id).await?;
    expect_i64(&second_completed_scan, "pairs_found", 1)?;
    expect_i64(&second_completed_scan, "contained_pairs", 1)?;
    let fingerprint_times_after: Vec<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT computed_at FROM book_fingerprints ORDER BY book_id")
            .fetch_all(&pool)
            .await
            .context("load fingerprint timestamps after incremental rescan")?;
    if fingerprint_times_after != fingerprint_times_before {
        bail!("unchanged books must reuse persisted fingerprints during an incremental rescan");
    }

    let unrelated_text = (1..=3)
        .map(|index| {
            format!(
                "第{index}章 独立故事\n{}",
                format!("这是一段只属于新增测试书籍{index}的独立正文，不应形成重复候选。")
                    .repeat(12)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    tokio::fs::write(
        library_root.join("incremental-unrelated.txt"),
        unrelated_text,
    )
    .await
    .context("write incrementally imported novel")?;
    let library_scan = client
        .post(format!("{origin}/api/libraries/{library_id}/scan"))
        .bearer_auth(access_token)
        .send()
        .await
        .context("trigger library import and incremental dedup scan")?;
    let library_scan = response_data(library_scan).await?;
    expect_i64(&library_scan, "new_books", 1)?;
    let incremental_scan_id = library_scan
        .get("dedup_scan_id")
        .and_then(Value::as_str)
        .context("library scan must return its incremental dedup scan id")?;
    let incremental_scan =
        wait_for_completed_scan(&client, &origin, access_token, incremental_scan_id).await?;
    expect_i64(&incremental_scan, "books_total", 1)?;
    expect_i64(&incremental_scan, "books_processed", 1)?;
    expect_i64(&incremental_scan, "chapters_processed", 3)?;
    expect_i64(&incremental_scan, "pairs_found", 0)?;

    let original_fingerprint_times: Vec<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT computed_at FROM book_fingerprints WHERE book_id = ANY($1) ORDER BY book_id",
    )
    .bind(vec![
        Uuid::parse_str(SHORT_BOOK_ID)?,
        Uuid::parse_str(LONG_BOOK_ID)?,
    ])
    .fetch_all(&pool)
    .await
    .context("load original fingerprints after incremental import")?;
    if original_fingerprint_times != fingerprint_times_before {
        bail!("single-book incremental import must not recompute unchanged library fingerprints");
    }
    let fingerprint_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM book_fingerprints")
        .fetch_one(&pool)
        .await
        .context("count fingerprints after incremental import")?;
    if fingerprint_count != 3 {
        bail!("incremental import must fingerprint exactly the new book; got {fingerprint_count}");
    }

    let pairs = client
        .get(format!(
            "{origin}/api/duplicates?library_id={library_id}&relation=contained_version"
        ))
        .bearer_auth(access_token)
        .send()
        .await
        .context("list contained duplicate pairs through API")?;
    let pair_page = response_data(pairs).await?;
    expect_i64(&pair_page, "total", 1)?;
    let pair_list = pair_page
        .get("items")
        .and_then(Value::as_array)
        .context("duplicates response data.items must be an array")?;
    if pair_list.len() != 1 {
        bail!(
            "expected exactly one contained pair, got {}",
            pair_list.len()
        );
    }
    let pair = &pair_list[0];
    expect_str(pair, "relation", "contained_version")?;
    expect_str(pair, "contained_book_id", SHORT_BOOK_ID)?;
    expect_str(pair, "recommended_primary_id", LONG_BOOK_ID)?;
    expect_nested_str(pair, &["book_a", "id"], SHORT_BOOK_ID)?;
    expect_nested_str(pair, &["book_b", "id"], LONG_BOOK_ID)?;
    expect_i64(pair, "shared_chapters", 10)?;
    expect_i64(pair, "longest_contiguous_run", 10)?;
    expect_approx(pair, "coverage_a", 1.0, 0.000_001)?;
    expect_approx(pair, "coverage_b", 10.0 / 15.0, 0.000_001)?;
    expect_approx(pair, "order_score", 1.0, 0.000_001)?;

    let pair_id = pair
        .get("id")
        .and_then(Value::as_str)
        .context("pair response missing id")?;
    assert_changed_content_stales_pair_and_rejects_resolution(
        &client,
        &origin,
        access_token,
        &pool,
        library_id,
        Uuid::parse_str(pair_id)?,
    )
    .await?;
    let detail = client
        .get(format!("{origin}/api/duplicates/{pair_id}"))
        .bearer_auth(access_token)
        .send()
        .await
        .context("load duplicate pair evidence through API")?;
    let detail = response_data(detail).await?;
    let recommendation = detail
        .pointer("/evidence/primary_recommendation")
        .context("pair detail missing primary recommendation evidence")?;
    if recommendation
        .get("reader_assets_considered")
        .and_then(Value::as_bool)
        != Some(false)
    {
        bail!("global duplicate recommendations must not consider private reader assets");
    }
    for side in ["book_a", "book_b"] {
        let side_evidence = recommendation
            .get(side)
            .and_then(Value::as_object)
            .with_context(|| format!("primary recommendation missing {side} evidence"))?;
        if side_evidence.contains_key("reader_assets")
            || side_evidence.contains_key("reader_asset_count")
        {
            bail!("primary recommendation evidence must not expose global reader asset counts");
        }
        if !side_evidence.contains_key("unique_informative_chapters")
            || !side_evidence.contains_key("repeated_informative_chapters")
            || !side_evidence.contains_key("text_integrity_score")
        {
            bail!("primary recommendation must expose explainable content and integrity signals");
        }
    }
    let matches = detail
        .get("chapter_matches")
        .and_then(Value::as_array)
        .context("pair detail missing chapter_matches")?;
    expect_i64(&detail, "chapter_matches_total", 10)?;
    if matches.len() != 10 {
        bail!("expected 10 chapter mappings, got {}", matches.len());
    }
    for (index, chapter_match) in matches.iter().enumerate() {
        expect_i64(chapter_match, "chapter_a_index", index as i64)?;
        expect_i64(chapter_match, "chapter_b_index", index as i64)?;
        expect_approx(chapter_match, "similarity", 1.0, 0.000_001)?;
        if chapter_match
            .get("chapter_a_id")
            .and_then(Value::as_str)
            .is_none()
            || chapter_match
                .get("chapter_b_id")
                .and_then(Value::as_str)
                .is_none()
        {
            bail!("chapter mapping {index} must reference both real chapters");
        }
    }

    let first_match_id = matches[0]
        .get("id")
        .and_then(Value::as_str)
        .context("first chapter match missing id")?;
    let diff = client
        .get(format!(
            "{origin}/api/duplicates/{pair_id}/matches/{first_match_id}/diff"
        ))
        .bearer_auth(access_token)
        .send()
        .await
        .context("load matched chapter diff through API")?;
    let diff = response_data(diff).await?;
    expect_approx(&diff, "ratio", 1.0, 0.000_001)?;
    let changes = diff
        .get("changes")
        .and_then(Value::as_array)
        .context("chapter diff missing changes")?;
    if changes.is_empty()
        || changes
            .iter()
            .any(|change| change.get("tag").and_then(Value::as_str) != Some("equal"))
    {
        bail!("identical mapped chapters must produce an all-equal, non-empty diff");
    }
    if diff.get("truncated").and_then(Value::as_bool) != Some(false) {
        bail!("short identical chapter diff must not be truncated");
    }

    let unreliable_chapter_id = matches[0]
        .get("chapter_a_id")
        .and_then(Value::as_str)
        .context("first source chapter id is missing")
        .and_then(|value| Uuid::parse_str(value).context("first source chapter id is invalid"))?;
    let reliable_chapter_id = matches[1]
        .get("chapter_a_id")
        .and_then(Value::as_str)
        .context("second source chapter id is missing")
        .and_then(|value| Uuid::parse_str(value).context("second source chapter id is invalid"))?;
    sqlx::query(
        "UPDATE duplicate_chapter_matches SET match_type = 'winnowing', similarity = 0.9 WHERE id = $1",
    )
    .bind(Uuid::parse_str(first_match_id)?)
    .execute(&pool)
    .await
    .context("mark one chapter mapping approximate for resolution safety test")?;
    seed_resolution_artifacts(
        &pool,
        user_id,
        unreliable_chapter_id,
        reliable_chapter_id,
        &temp_root.join("short-version-source.txt"),
    )
    .await?;

    let resolution = client
        .post(format!("{origin}/api/duplicates/{pair_id}/resolve"))
        .bearer_auth(access_token)
        .json(&json!({ "action": "keep_b" }))
        .send()
        .await
        .context("resolve contained version through API")?;
    let resolution = response_data(resolution).await?;
    expect_str(&resolution, "status", "confirmed")?;
    expect_str(&resolution, "primary_book_id", LONG_BOOK_ID)?;
    expect_str(&resolution, "secondary_book_id", SHORT_BOOK_ID)?;
    if resolution
        .get("source_file_deleted")
        .and_then(Value::as_bool)
        != Some(false)
    {
        bail!("resolving a duplicate must not delete its source file");
    }
    expect_i64(&resolution, "library_links_copied", 3)?;
    if resolution.get("library_links_moved").is_some() {
        bail!("resolution API must describe preserved source links as copied, not moved");
    }
    let cleanup_task_id = resolution
        .get("index_cleanup_task_id")
        .and_then(Value::as_str)
        .context("resolution must enqueue retryable index cleanup")
        .and_then(|value| Uuid::parse_str(value).context("cleanup task id is invalid"))?;
    assert_resolution_safety(&pool, Uuid::parse_str(pair_id)?, cleanup_task_id).await?;
    assert_unchanged_resolution_survives_full_scan(
        &client,
        &origin,
        access_token,
        &pool,
        library_id,
        Uuid::parse_str(pair_id)?,
    )
    .await?;
    assert_scan_retry_lifecycle(&client, &origin, access_token, &pool, library_id, user_id).await?;

    pool.close().await;
    Ok(())
}

async fn assert_changed_content_stales_pair_and_rejects_resolution(
    client: &Client,
    origin: &str,
    access_token: &str,
    pool: &PgPool,
    library_id: Uuid,
    pair_id: Uuid,
) -> Result<()> {
    let short_book_id = Uuid::parse_str(SHORT_BOOK_ID)?;
    let original_content: String =
        sqlx::query_scalar("SELECT content FROM chapters WHERE book_id = $1 AND chapter_index = 2")
            .bind(short_book_id)
            .fetch_one(pool)
            .await
            .context("load chapter before stale-resolution test")?;

    // First prove the production trigger closes the visibility gap
    // immediately. Keep the probe in a transaction so rolling it back also
    // restores the pair and fingerprint rows for the independent resolver
    // defence-in-depth test below.
    let mut trigger_probe = pool
        .begin()
        .await
        .context("begin immediate invalidation trigger probe")?;
    sqlx::query(
        "UPDATE chapters SET content = content || $2 WHERE book_id = $1 AND chapter_index = 2",
    )
    .bind(short_book_id)
    .bind("\n触发器必须立即隐藏旧去重证据。")
    .execute(&mut *trigger_probe)
    .await
    .context("change chapter while invalidation trigger is enabled")?;
    let trigger_marked_stale: bool =
        sqlx::query_scalar("SELECT stale FROM duplicate_pairs WHERE id = $1")
            .bind(pair_id)
            .fetch_one(&mut *trigger_probe)
            .await
            .context("inspect pair invalidated by chapter trigger")?;
    let remaining_cached_books: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM book_fingerprints WHERE book_id = $1")
            .bind(short_book_id)
            .fetch_one(&mut *trigger_probe)
            .await
            .context("inspect book fingerprint invalidated by chapter trigger")?;
    let remaining_cached_chapters: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM chapter_fingerprints WHERE book_id = $1")
            .bind(short_book_id)
            .fetch_one(&mut *trigger_probe)
            .await
            .context("inspect chapter fingerprints invalidated by chapter trigger")?;
    if !trigger_marked_stale || remaining_cached_books != 0 || remaining_cached_chapters != 0 {
        bail!(
            "chapter content trigger must stale the pair and clear its caches immediately; got stale={trigger_marked_stale}, book_fingerprints={remaining_cached_books}, chapter_fingerprints={remaining_cached_chapters}"
        );
    }
    trigger_probe
        .rollback()
        .await
        .context("rollback immediate invalidation trigger probe")?;

    let mut file_hash_probe = pool
        .begin()
        .await
        .context("begin exact-file evidence trigger probe")?;
    sqlx::query("UPDATE books SET file_hash = file_hash || '-changed' WHERE id = $1")
        .bind(short_book_id)
        .execute(&mut *file_hash_probe)
        .await
        .context("change source file hash while invalidation trigger is enabled")?;
    let file_hash_marked_stale: bool =
        sqlx::query_scalar("SELECT stale FROM duplicate_pairs WHERE id = $1")
            .bind(pair_id)
            .fetch_one(&mut *file_hash_probe)
            .await
            .context("inspect pair invalidated by file hash trigger")?;
    if !file_hash_marked_stale {
        bail!("source file hash trigger must stale persisted duplicate evidence immediately");
    }
    file_hash_probe
        .rollback()
        .await
        .context("rollback exact-file evidence trigger probe")?;

    // The production trigger is the first line of defence. Temporarily bypass
    // it in this disposable database so this test independently proves that
    // the resolution transaction does not trust an old fingerprint cache.
    let mut stale_fixture = pool
        .begin()
        .await
        .context("begin stale fingerprint fixture transaction")?;
    sqlx::query("ALTER TABLE chapters DISABLE TRIGGER trg_invalidate_novel_dedup_evidence")
        .execute(&mut *stale_fixture)
        .await
        .context("temporarily disable dedup invalidation trigger")?;
    sqlx::query(
        "UPDATE chapters SET content = content || $2 WHERE book_id = $1 AND chapter_index = 2",
    )
    .bind(short_book_id)
    .bind("\n这段正文是在扫描后才发生的安全测试变更。")
    .execute(&mut *stale_fixture)
    .await
    .context("change chapter without refreshing dedup fingerprints")?;
    sqlx::query("ALTER TABLE chapters ENABLE TRIGGER trg_invalidate_novel_dedup_evidence")
        .execute(&mut *stale_fixture)
        .await
        .context("restore dedup invalidation trigger")?;
    stale_fixture
        .commit()
        .await
        .context("commit stale fingerprint fixture")?;
    let stale_before_resolution: bool =
        sqlx::query_scalar("SELECT stale FROM duplicate_pairs WHERE id = $1")
            .bind(pair_id)
            .fetch_one(pool)
            .await
            .context("verify stale fixture bypassed immediate invalidation")?;
    let cached_fingerprint_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM chapter_fingerprints WHERE book_id = $1")
            .bind(short_book_id)
            .fetch_one(pool)
            .await
            .context("verify stale fixture retained the old chapter fingerprints")?;
    if stale_before_resolution || cached_fingerprint_count != 10 {
        bail!(
            "fixture must leave a seemingly fresh pair with ten stale fingerprints; got stale={stale_before_resolution}, fingerprints={cached_fingerprint_count}"
        );
    }

    let rejected = client
        .post(format!("{origin}/api/duplicates/{pair_id}/resolve"))
        .bearer_auth(access_token)
        .json(&json!({ "action": "keep_b" }))
        .send()
        .await
        .context("attempt resolution against stale chapter evidence")?;
    if rejected.status().as_u16() != 400 {
        let status = rejected.status();
        let body = rejected.text().await.unwrap_or_default();
        bail!("changed content must reject resolution with 400; got {status}: {body}");
    }
    let rejected_body: Value = rejected
        .json()
        .await
        .context("decode stale resolution rejection")?;
    let message = rejected_body
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !message.contains("stale") || !message.contains("rescan") {
        bail!("stale resolution rejection must explain that a rescan is required: {rejected_body}");
    }

    let (stale, review_status, resolved): (bool, String, bool) =
        sqlx::query_as("SELECT stale, review_status, resolved FROM duplicate_pairs WHERE id = $1")
            .bind(pair_id)
            .fetch_one(pool)
            .await
            .context("load pair after stale resolution rejection")?;
    if !stale || review_status != "pending" || resolved {
        bail!(
            "changed content must persistently stale an unresolved pair; got stale={stale}, review_status={review_status:?}, resolved={resolved}"
        );
    }
    let changed_books: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM books WHERE id = ANY($1) AND (work_id IS NOT NULL OR status::text <> 'ready')",
    )
    .bind(vec![
        Uuid::parse_str(SHORT_BOOK_ID)?,
        Uuid::parse_str(LONG_BOOK_ID)?,
    ])
    .fetch_one(pool)
    .await
    .context("verify stale rejection has no book/work side effects")?;
    if changed_books != 0 {
        bail!("stale evidence rejection must not group or archive either book");
    }

    sqlx::query("UPDATE chapters SET content = $2 WHERE book_id = $1 AND chapter_index = 2")
        .bind(short_book_id)
        .bind(original_content)
        .execute(pool)
        .await
        .context("restore chapter after stale-resolution test")?;
    let rescan = client
        .post(format!("{origin}/api/duplicates/scans"))
        .bearer_auth(access_token)
        .json(&json!({
            "library_id": library_id,
            "include_semantic": false
        }))
        .send()
        .await
        .context("rescan restored content after stale resolution rejection")?;
    let rescan = response_data(rescan).await?;
    let scan_id = rescan
        .get("id")
        .and_then(Value::as_str)
        .context("stale recovery scan response missing id")?;
    let completed = wait_for_completed_scan(client, origin, access_token, scan_id).await?;
    expect_i64(&completed, "pairs_found", 1)?;
    let stale_after_rescan: bool =
        sqlx::query_scalar("SELECT stale FROM duplicate_pairs WHERE id = $1")
            .bind(pair_id)
            .fetch_one(pool)
            .await
            .context("verify restored pair is fresh after rescan")?;
    if stale_after_rescan {
        bail!("a successful rescan of restored content must make the pair reviewable again");
    }
    Ok(())
}

async fn assert_scan_retry_lifecycle(
    client: &Client,
    origin: &str,
    access_token: &str,
    pool: &PgPool,
    library_id: Uuid,
    user_id: Uuid,
) -> Result<()> {
    let retry_task_id = Uuid::now_v7();
    let retry_scan_id = Uuid::now_v7();
    let retry_payload = json!({
        "operation": "scan",
        "scan_run_id": retry_scan_id,
        "library_id": library_id,
        "include_semantic": false,
        "target_book_ids": ["not-a-uuid"]
    });
    let mut retry_tx = pool
        .begin()
        .await
        .context("begin retry lifecycle fixture transaction")?;
    sqlx::query(
        r#"INSERT INTO tasks
           (id, kind, status, priority, payload, category, retry_count,
            max_retries, scheduled_at)
           VALUES ($1, 'deduplicate'::task_kind, 'queued'::task_status,
                   '1'::task_priority, $2, 'preprocess', 0, 1, NOW())"#,
    )
    .bind(retry_task_id)
    .bind(retry_payload)
    .execute(&mut *retry_tx)
    .await
    .context("insert retry lifecycle task")?;
    sqlx::query(
        r#"INSERT INTO dedup_scan_runs
           (id, library_id, requested_by, task_id, include_semantic,
            algorithm_version, status)
           VALUES ($1, $2, $3, $4, FALSE, 3, 'queued')"#,
    )
    .bind(retry_scan_id)
    .bind(library_id)
    .bind(user_id)
    .bind(retry_task_id)
    .execute(&mut *retry_tx)
    .await
    .context("insert retry lifecycle scan")?;
    retry_tx
        .commit()
        .await
        .context("commit retry lifecycle fixture transaction")?;

    let queued_scan =
        wait_for_scan_status_with_error(client, origin, access_token, retry_scan_id, "queued")
            .await?;
    expect_str(&queued_scan, "progress_message", "retrying")?;
    if queued_scan
        .get("completed_at")
        .is_some_and(|value| !value.is_null())
    {
        bail!("a retryable scan failure must not set completed_at");
    }
    let retry_state: (String, i32, i32) =
        sqlx::query_as("SELECT status::text, retry_count, max_retries FROM tasks WHERE id = $1")
            .bind(retry_task_id)
            .fetch_one(pool)
            .await
            .context("load retryable task state")?;
    if retry_state != ("queued".to_string(), 1, 1) {
        bail!("first failure must queue exactly one retry; got {retry_state:?}");
    }

    sqlx::query("UPDATE tasks SET scheduled_at = NOW() WHERE id = $1")
        .bind(retry_task_id)
        .execute(pool)
        .await
        .context("make final retry immediately runnable")?;
    let failed_scan =
        wait_for_scan_status_with_error(client, origin, access_token, retry_scan_id, "failed")
            .await?;
    expect_str(&failed_scan, "progress_message", "failed")?;
    if failed_scan.get("completed_at").is_none_or(Value::is_null) {
        bail!("a dead-lettered scan must set completed_at");
    }
    let dead_letter_state: (String, i32, i32) =
        sqlx::query_as("SELECT status::text, retry_count, max_retries FROM tasks WHERE id = $1")
            .bind(retry_task_id)
            .fetch_one(pool)
            .await
            .context("load dead-letter task state")?;
    if dead_letter_state != ("dead_letter".to_string(), 1, 1) {
        bail!("retry exhaustion must dead-letter the task; got {dead_letter_state:?}");
    }

    let maintenance_task_id = Uuid::now_v7();
    let completed_scan_id = Uuid::now_v7();
    let maintenance_payload = json!({
        "operation": "cleanup_secondary_indexes",
        "secondary_book_id": "not-a-uuid",
        "primary_book_id": LONG_BOOK_ID
    });
    let mut maintenance_tx = pool
        .begin()
        .await
        .context("begin maintenance fixture transaction")?;
    sqlx::query(
        r#"INSERT INTO tasks
           (id, kind, status, priority, payload, category, retry_count,
            max_retries, scheduled_at)
           VALUES ($1, 'deduplicate'::task_kind, 'queued'::task_status,
                   '1'::task_priority, $2, 'maintenance', 0, 0, NOW())"#,
    )
    .bind(maintenance_task_id)
    .bind(maintenance_payload)
    .execute(&mut *maintenance_tx)
    .await
    .context("insert failing maintenance task")?;
    sqlx::query(
        r#"INSERT INTO dedup_scan_runs
           (id, library_id, requested_by, task_id, include_semantic,
            algorithm_version, status, progress, completed_at)
           VALUES ($1, $2, $3, $4, FALSE, 3, 'completed', 100, NOW())"#,
    )
    .bind(completed_scan_id)
    .bind(library_id)
    .bind(user_id)
    .bind(maintenance_task_id)
    .execute(&mut *maintenance_tx)
    .await
    .context("insert completed scan linked to maintenance task")?;
    maintenance_tx
        .commit()
        .await
        .context("commit maintenance fixture transaction")?;

    wait_for_task_status(pool, maintenance_task_id, "dead_letter").await?;
    let completed_scan = load_scan(client, origin, access_token, completed_scan_id).await?;
    expect_str(&completed_scan, "status", "completed")?;
    if completed_scan
        .get("error_message")
        .is_some_and(|value| !value.is_null())
    {
        bail!("a maintenance task failure must not mutate a scan error state");
    }

    Ok(())
}

async fn wait_for_scan_status_with_error(
    client: &Client,
    origin: &str,
    access_token: &str,
    scan_id: Uuid,
    expected_status: &str,
) -> Result<Value> {
    let mut last_scan = Value::Null;
    for _ in 0..120 {
        last_scan = load_scan(client, origin, access_token, scan_id).await?;
        let has_error = last_scan
            .get("error_message")
            .and_then(Value::as_str)
            .is_some_and(|error| !error.is_empty());
        if last_scan.get("status").and_then(Value::as_str) == Some(expected_status) && has_error {
            return Ok(last_scan);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    bail!("scan {scan_id} did not reach {expected_status} with an error: {last_scan}")
}

async fn load_scan(
    client: &Client,
    origin: &str,
    access_token: &str,
    scan_id: Uuid,
) -> Result<Value> {
    let response = client
        .get(format!("{origin}/api/duplicates/scans/{scan_id}"))
        .bearer_auth(access_token)
        .send()
        .await
        .context("load duplicate scan through API")?;
    response_data(response).await
}

async fn wait_for_task_status(pool: &PgPool, task_id: Uuid, expected: &str) -> Result<()> {
    let mut last_status = String::new();
    for _ in 0..120 {
        last_status =
            sqlx::query_scalar::<_, String>("SELECT status::text FROM tasks WHERE id = $1")
                .bind(task_id)
                .fetch_one(pool)
                .await
                .context("poll maintenance task state")?;
        if last_status == expected {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    bail!("task {task_id} did not reach {expected}; last status was {last_status}")
}

async fn seed_grouped_reader_assets(
    pool: &PgPool,
    user_id: Uuid,
    source_chapter_id: Uuid,
) -> Result<()> {
    let short_book_id = Uuid::parse_str(SHORT_BOOK_ID)?;
    let source_chapter_index: i32 =
        sqlx::query_scalar("SELECT chapter_index FROM chapters WHERE id = $1 AND book_id = $2")
            .bind(source_chapter_id)
            .bind(short_book_id)
            .fetch_one(pool)
            .await
            .context("load grouped source chapter index")?;

    sqlx::query(
        r#"INSERT INTO annotations
           (id, user_id, book_id, chapter_id, chapter_index, selected_text,
            note, start_offset, end_offset)
           VALUES ($1, $2, $3, $4, $5, $6, 'grouped mapping must not migrate', 0, 8)"#,
    )
    .bind(Uuid::parse_str(APPROXIMATE_ANNOTATION_ID)?)
    .bind(user_id)
    .bind(short_book_id)
    .bind(source_chapter_id)
    .bind(source_chapter_index)
    .bind(APPROXIMATE_ANNOTATION_TEXT)
    .execute(pool)
    .await
    .context("insert annotation on grouped source chapter")?;
    sqlx::query(
        r#"INSERT INTO bookmarks
           (id, user_id, book_id, chapter_id, chapter_index, position, title)
           VALUES ($1, $2, $3, $4, $5, 0.25, $6)"#,
    )
    .bind(Uuid::parse_str(APPROXIMATE_BOOKMARK_ID)?)
    .bind(user_id)
    .bind(short_book_id)
    .bind(source_chapter_id)
    .bind(source_chapter_index)
    .bind(APPROXIMATE_BOOKMARK_TITLE)
    .execute(pool)
    .await
    .context("insert bookmark on grouped source chapter")?;
    sqlx::query(
        r#"INSERT INTO reading_progress
           (id, user_id, book_id, chapter_id, chapter_index, progress,
            current_chapter, reading_time_secs)
           VALUES ($1, $2, $3, $4, $5, 0.42, $5, 321)"#,
    )
    .bind(Uuid::parse_str(APPROXIMATE_PROGRESS_ID)?)
    .bind(user_id)
    .bind(short_book_id)
    .bind(source_chapter_id)
    .bind(source_chapter_index)
    .execute(pool)
    .await
    .context("insert reading progress on grouped source chapter")?;

    Ok(())
}

async fn assert_grouped_reader_assets_stayed_on_source(
    pool: &PgPool,
    source_chapter_id: Uuid,
) -> Result<()> {
    let short_book_id = Uuid::parse_str(SHORT_BOOK_ID)?;
    let locations: Vec<(String, Uuid, Uuid)> = sqlx::query_as(
        r#"SELECT 'annotation', book_id, chapter_id
           FROM annotations WHERE id = $1
           UNION ALL
           SELECT 'bookmark', book_id, chapter_id
           FROM bookmarks WHERE id = $2
           UNION ALL
           SELECT 'progress', book_id, chapter_id
           FROM reading_progress WHERE id = $3
           ORDER BY 1"#,
    )
    .bind(Uuid::parse_str(APPROXIMATE_ANNOTATION_ID)?)
    .bind(Uuid::parse_str(APPROXIMATE_BOOKMARK_ID)?)
    .bind(Uuid::parse_str(APPROXIMATE_PROGRESS_ID)?)
    .fetch_all(pool)
    .await
    .context("load grouped reader assets after resolution")?;
    let expected = vec![
        ("annotation".to_string(), short_book_id, source_chapter_id),
        ("bookmark".to_string(), short_book_id, source_chapter_id),
        ("progress".to_string(), short_book_id, source_chapter_id),
    ];
    if locations != expected {
        bail!(
            "grouped winnowing mappings must not migrate annotations, bookmarks or progress; got {locations:?}"
        );
    }
    Ok(())
}

async fn seed_resolution_artifacts(
    pool: &PgPool,
    user_id: Uuid,
    unreliable_chapter_id: Uuid,
    reliable_chapter_id: Uuid,
    source_path: &Path,
) -> Result<()> {
    let short_book_id = Uuid::parse_str(SHORT_BOOK_ID)?;
    let approximate_annotation_id = Uuid::parse_str(APPROXIMATE_ANNOTATION_ID)?;
    let reliable_annotation_id = Uuid::parse_str(RELIABLE_ANNOTATION_ID)?;
    let approximate_bookmark_id = Uuid::parse_str(APPROXIMATE_BOOKMARK_ID)?;
    let approximate_progress_id = Uuid::parse_str(APPROXIMATE_PROGRESS_ID)?;
    let (unreliable_index, reliable_index): (i32, i32) = sqlx::query_as(
        r#"SELECT
               (SELECT chapter_index FROM chapters WHERE id = $1 AND book_id = $3),
               (SELECT chapter_index FROM chapters WHERE id = $2 AND book_id = $3)"#,
    )
    .bind(unreliable_chapter_id)
    .bind(reliable_chapter_id)
    .bind(short_book_id)
    .fetch_one(pool)
    .await
    .context("load source chapter indexes for resolution artifacts")?;

    sqlx::query(
        r#"INSERT INTO annotations
           (id, user_id, book_id, chapter_id, chapter_index, selected_text,
            note, start_offset, end_offset)
           VALUES ($1, $2, $3, $4, $5, $6, 'dedup e2e approximate mapping', 0, 8)"#,
    )
    .bind(approximate_annotation_id)
    .bind(user_id)
    .bind(short_book_id)
    .bind(unreliable_chapter_id)
    .bind(unreliable_index)
    .bind(APPROXIMATE_ANNOTATION_TEXT)
    .execute(pool)
    .await
    .context("insert annotation on approximate chapter mapping")?;

    sqlx::query(
        r#"INSERT INTO bookmarks
           (id, user_id, book_id, chapter_id, chapter_index, position, title)
           VALUES ($1, $2, $3, $4, $5, 0.25, $6)"#,
    )
    .bind(approximate_bookmark_id)
    .bind(user_id)
    .bind(short_book_id)
    .bind(unreliable_chapter_id)
    .bind(unreliable_index)
    .bind(APPROXIMATE_BOOKMARK_TITLE)
    .execute(pool)
    .await
    .context("insert bookmark on approximate chapter mapping")?;

    sqlx::query(
        r#"INSERT INTO reading_progress
           (id, user_id, book_id, chapter_id, chapter_index, progress,
            current_chapter, reading_time_secs)
           VALUES ($1, $2, $3, $4, $5, 0.42, $5, 321)"#,
    )
    .bind(approximate_progress_id)
    .bind(user_id)
    .bind(short_book_id)
    .bind(unreliable_chapter_id)
    .bind(unreliable_index)
    .execute(pool)
    .await
    .context("insert reading progress on approximate chapter mapping")?;

    sqlx::query(
        r#"INSERT INTO annotations
           (id, user_id, book_id, chapter_id, chapter_index, selected_text,
            note, start_offset, end_offset)
           VALUES ($1, $2, $3, $4, $5, $6, 'dedup e2e reliable mapping', 9, 18)"#,
    )
    .bind(reliable_annotation_id)
    .bind(user_id)
    .bind(short_book_id)
    .bind(reliable_chapter_id)
    .bind(reliable_index)
    .bind(RELIABLE_ANNOTATION_TEXT)
    .execute(pool)
    .await
    .context("insert annotation on reliable chapter mapping")?;

    let progress_conflict_user_id = Uuid::parse_str(PROGRESS_CONFLICT_USER_ID)?;
    let short_complete_progress_id = Uuid::parse_str(SHORT_COMPLETE_PROGRESS_ID)?;
    let long_later_progress_id = Uuid::parse_str(LONG_LATER_PROGRESS_ID)?;
    let long_book_id = Uuid::parse_str(LONG_BOOK_ID)?;
    let (short_last_chapter_id, long_later_chapter_id): (Uuid, Uuid) = sqlx::query_as(
        r#"SELECT
               (SELECT id FROM chapters WHERE book_id = $1 AND chapter_index = 9),
               (SELECT id FROM chapters WHERE book_id = $2 AND chapter_index = 12)"#,
    )
    .bind(short_book_id)
    .bind(long_book_id)
    .fetch_one(pool)
    .await
    .context("load chapters for cross-version progress conflict")?;
    sqlx::query(
        r#"INSERT INTO users (id, username, password_hash, display_name)
           VALUES ($1, 'dedup-progress-conflict', 'not-used-in-e2e', 'Dedup Progress Conflict')"#,
    )
    .bind(progress_conflict_user_id)
    .execute(pool)
    .await
    .context("insert reader for cross-version progress conflict")?;
    sqlx::query(
        r#"INSERT INTO reading_progress
           (id, user_id, book_id, chapter_id, chapter_index, progress,
            current_chapter, scroll_position, reading_time_secs)
           VALUES ($1, $2, $3, $4, 9, 1.0, 9, 1.0, 111),
                  ($5, $2, $6, $7, 12, $8, 12, 0.5, 222)"#,
    )
    .bind(short_complete_progress_id)
    .bind(progress_conflict_user_id)
    .bind(short_book_id)
    .bind(short_last_chapter_id)
    .bind(long_later_progress_id)
    .bind(long_book_id)
    .bind(long_later_chapter_id)
    .bind(12.5 / 15.0)
    .execute(pool)
    .await
    .context("insert incomparable short/long full-book progress values")?;

    tokio::fs::write(
        source_path,
        b"dedup e2e source file must survive resolution",
    )
    .await
    .context("create source file for non-destructive resolution assertion")?;
    sqlx::query("UPDATE books SET file_path = $2 WHERE id = $1")
        .bind(short_book_id)
        .bind(path_string(source_path))
        .execute(pool)
        .await
        .context("point short version at isolated source file")?;

    seed_version_provenance(pool, short_book_id, long_book_id).await?;

    Ok(())
}

async fn seed_version_provenance(
    pool: &PgPool,
    short_book_id: Uuid,
    long_book_id: Uuid,
) -> Result<()> {
    let collection_id = Uuid::parse_str(PROVENANCE_COLLECTION_ID)?;
    let shelf_id = Uuid::parse_str(PROVENANCE_SHELF_ID)?;
    let tag_id = Uuid::parse_str(PROVENANCE_TAG_ID)?;
    let series_id = Uuid::parse_str(PROVENANCE_SERIES_ID)?;
    let translator_id = Uuid::parse_str(PROVENANCE_TRANSLATOR_ID)?;
    let editor_id = Uuid::parse_str(PROVENANCE_EDITOR_ID)?;
    let library_id: Uuid = sqlx::query_scalar("SELECT library_id FROM books WHERE id = $1")
        .bind(short_book_id)
        .fetch_one(pool)
        .await
        .context("load library for version provenance fixtures")?;

    sqlx::query("INSERT INTO collections (id, name) VALUES ($1, '次版本来源合集')")
        .bind(collection_id)
        .execute(pool)
        .await
        .context("insert source-version collection")?;
    sqlx::query("INSERT INTO collection_books (collection_id, book_id) VALUES ($1, $2)")
        .bind(collection_id)
        .bind(short_book_id)
        .execute(pool)
        .await
        .context("link source version to collection")?;

    sqlx::query("INSERT INTO shelves (id, name) VALUES ($1, '次版本来源书架')")
        .bind(shelf_id)
        .execute(pool)
        .await
        .context("insert source-version shelf")?;
    sqlx::query("INSERT INTO shelf_books (shelf_id, book_id) VALUES ($1, $2)")
        .bind(shelf_id)
        .bind(short_book_id)
        .execute(pool)
        .await
        .context("link source version to shelf")?;

    sqlx::query("INSERT INTO tags (id, name) VALUES ($1, '次版本来源标签')")
        .bind(tag_id)
        .execute(pool)
        .await
        .context("insert source-version tag")?;
    sqlx::query("INSERT INTO book_tags (book_id, tag_id) VALUES ($1, $2)")
        .bind(short_book_id)
        .bind(tag_id)
        .execute(pool)
        .await
        .context("tag source version")?;

    sqlx::query(
        r#"INSERT INTO series
           (id, library_id, name, sort_name, folder_path)
           VALUES ($1, $2, '短版专属丛书', '短版专属丛书', '/dedup-e2e/short-edition-series')"#,
    )
    .bind(series_id)
    .bind(library_id)
    .execute(pool)
    .await
    .context("insert source-version series")?;
    sqlx::query(
        r#"INSERT INTO series_books (series_id, book_id, sort_order, volume_label)
           VALUES ($1, $2, 7.0, '测试短版')"#,
    )
    .bind(series_id)
    .bind(short_book_id)
    .execute(pool)
    .await
    .context("link source version to edition-specific series")?;

    sqlx::query(
        r#"INSERT INTO persons (id, name, sort_name, role)
           VALUES ($1, '短版译者', '短版译者', 'translator'),
                  ($2, '短版编辑', '短版编辑', 'editor')"#,
    )
    .bind(translator_id)
    .bind(editor_id)
    .execute(pool)
    .await
    .context("insert source-version contributors")?;
    sqlx::query(
        r#"INSERT INTO book_persons (book_id, person_id, role)
           VALUES ($1, $2, 'translator'), ($1, $3, 'editor')"#,
    )
    .bind(short_book_id)
    .bind(translator_id)
    .bind(editor_id)
    .execute(pool)
    .await
    .context("link source version to edition-specific contributors")?;

    sqlx::query(
        r#"INSERT INTO book_ratings (book_id, rating, review)
           VALUES ($1, 6.0, '短版独立评分'), ($2, 9.0, '长版独立评分')"#,
    )
    .bind(short_book_id)
    .bind(long_book_id)
    .execute(pool)
    .await
    .context("insert conflicting version-specific ratings")?;

    Ok(())
}

async fn assert_resolution_safety(
    pool: &PgPool,
    pair_id: Uuid,
    cleanup_task_id: Uuid,
) -> Result<()> {
    let short_book_id = Uuid::parse_str(SHORT_BOOK_ID)?;
    let long_book_id = Uuid::parse_str(LONG_BOOK_ID)?;
    let approximate_annotation_id = Uuid::parse_str(APPROXIMATE_ANNOTATION_ID)?;
    let reliable_annotation_id = Uuid::parse_str(RELIABLE_ANNOTATION_ID)?;
    let approximate_bookmark_id = Uuid::parse_str(APPROXIMATE_BOOKMARK_ID)?;
    let approximate_progress_id = Uuid::parse_str(APPROXIMATE_PROGRESS_ID)?;
    let (approximate_source_id, approximate_target_id): (Option<Uuid>, Option<Uuid>) =
        sqlx::query_as(
            r#"SELECT chapter_a_id, chapter_b_id
               FROM duplicate_chapter_matches mapping
               JOIN duplicate_pairs pair ON pair.id = mapping.pair_id
               WHERE pair.book_a_id = $1 AND pair.book_b_id = $2
                 AND mapping.chapter_a_index = 0"#,
        )
        .bind(short_book_id)
        .bind(long_book_id)
        .fetch_one(pool)
        .await
        .context("load approximate chapter mapping after resolution")?;
    let approximate_source_id =
        approximate_source_id.context("approximate mapping must retain its source chapter")?;
    approximate_target_id.context("approximate mapping must retain its target chapter")?;

    let (reliable_source_id, reliable_target_id): (Option<Uuid>, Option<Uuid>) = sqlx::query_as(
        r#"SELECT chapter_a_id, chapter_b_id
           FROM duplicate_chapter_matches mapping
           JOIN duplicate_pairs pair ON pair.id = mapping.pair_id
           WHERE pair.book_a_id = $1 AND pair.book_b_id = $2
             AND mapping.chapter_a_index = 1"#,
    )
    .bind(short_book_id)
    .bind(long_book_id)
    .fetch_one(pool)
    .await
    .context("load reliable chapter mapping after resolution")?;
    reliable_source_id.context("reliable mapping must retain its source chapter")?;
    let reliable_target_id =
        reliable_target_id.context("reliable mapping must retain its target chapter")?;

    let approximate_annotation: (Uuid, Option<Uuid>, Option<i32>) =
        sqlx::query_as("SELECT book_id, chapter_id, chapter_index FROM annotations WHERE id = $1")
            .bind(approximate_annotation_id)
            .fetch_one(pool)
            .await
            .context("load annotation on approximate mapping after resolution")?;
    if approximate_annotation != (short_book_id, Some(approximate_source_id), Some(0)) {
        bail!(
            "annotation on approximate mapping must stay on short version/source chapter; got {approximate_annotation:?}"
        );
    }
    let approximate_annotation_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM annotations WHERE selected_text = $1")
            .bind(APPROXIMATE_ANNOTATION_TEXT)
            .fetch_one(pool)
            .await
            .context("count copies of annotation on approximate mapping")?;
    if approximate_annotation_count != 1 {
        bail!(
            "annotation on approximate mapping must not be copied; got {approximate_annotation_count} rows"
        );
    }

    let approximate_bookmark: (Uuid, Option<Uuid>, Option<i32>) =
        sqlx::query_as("SELECT book_id, chapter_id, chapter_index FROM bookmarks WHERE id = $1")
            .bind(approximate_bookmark_id)
            .fetch_one(pool)
            .await
            .context("load bookmark on approximate mapping after resolution")?;
    if approximate_bookmark != (short_book_id, Some(approximate_source_id), Some(0)) {
        bail!(
            "bookmark on approximate mapping must stay on short version/source chapter; got {approximate_bookmark:?}"
        );
    }
    let approximate_bookmark_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bookmarks WHERE title = $1")
            .bind(APPROXIMATE_BOOKMARK_TITLE)
            .fetch_one(pool)
            .await
            .context("count copies of bookmark on approximate mapping")?;
    if approximate_bookmark_count != 1 {
        bail!(
            "bookmark on approximate mapping must not be copied; got {approximate_bookmark_count} rows"
        );
    }

    let approximate_progress: (Uuid, Option<Uuid>, Option<i32>, i32, f64, i64) = sqlx::query_as(
        r#"SELECT book_id, chapter_id, chapter_index, current_chapter,
                  progress, reading_time_secs
           FROM reading_progress WHERE id = $1"#,
    )
    .bind(approximate_progress_id)
    .fetch_one(pool)
    .await
    .context("load reading progress on approximate mapping after resolution")?;
    if approximate_progress.0 != short_book_id
        || approximate_progress.1 != Some(approximate_source_id)
        || approximate_progress.2 != Some(0)
        || approximate_progress.3 != 0
        || (approximate_progress.4 - 0.42).abs() > f64::EPSILON
        || approximate_progress.5 != 321
    {
        bail!(
            "reading progress on approximate mapping must stay unchanged on short version/source chapter; got {approximate_progress:?}"
        );
    }
    let approximate_progress_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM reading_progress WHERE reading_time_secs = 321")
            .fetch_one(pool)
            .await
            .context("count copies of reading progress on approximate mapping")?;
    if approximate_progress_count != 1 {
        bail!(
            "reading progress on approximate mapping must not be copied; got {approximate_progress_count} rows"
        );
    }

    let progress_conflict_user_id = Uuid::parse_str(PROGRESS_CONFLICT_USER_ID)?;
    let long_later_progress_id = Uuid::parse_str(LONG_LATER_PROGRESS_ID)?;
    let long_later_chapter_id: Uuid =
        sqlx::query_scalar("SELECT id FROM chapters WHERE book_id = $1 AND chapter_index = 12")
            .bind(long_book_id)
            .fetch_one(pool)
            .await
            .context("load expected later long-version chapter")?;
    let merged_progress_rows: Vec<(
        Uuid,
        Uuid,
        Option<Uuid>,
        Option<i32>,
        i32,
        f64,
        Option<f64>,
        i64,
    )> = sqlx::query_as(
        r#"SELECT id, book_id, chapter_id, chapter_index, current_chapter,
                  progress, scroll_position, reading_time_secs
           FROM reading_progress
           WHERE user_id = $1
           ORDER BY book_id"#,
    )
    .bind(progress_conflict_user_id)
    .fetch_all(pool)
    .await
    .context("load safely merged cross-version progress")?;
    let expected_long_progress = 12.5 / 15.0;
    if merged_progress_rows.len() != 1
        || merged_progress_rows[0].0 != long_later_progress_id
        || merged_progress_rows[0].1 != long_book_id
        || merged_progress_rows[0].2 != Some(long_later_chapter_id)
        || merged_progress_rows[0].3 != Some(12)
        || merged_progress_rows[0].4 != 12
        || (merged_progress_rows[0].5 - expected_long_progress).abs() > 0.000_001
        || merged_progress_rows[0].6 != Some(0.5)
        || merged_progress_rows[0].7 != 333
    {
        bail!(
            "short-version 100% must not overwrite a later chapter in the longer version; got {merged_progress_rows:?}"
        );
    }

    let reliable_annotation: (Uuid, Option<Uuid>, Option<i32>) =
        sqlx::query_as("SELECT book_id, chapter_id, chapter_index FROM annotations WHERE id = $1")
            .bind(reliable_annotation_id)
            .fetch_one(pool)
            .await
            .context("load annotation on reliable mapping after resolution")?;
    if reliable_annotation != (long_book_id, Some(reliable_target_id), Some(1)) {
        bail!(
            "annotation on strict hash-equal mapping must move to the long version/target chapter; got {reliable_annotation:?}"
        );
    }
    let reliable_annotation_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM annotations WHERE selected_text = $1")
            .bind(RELIABLE_ANNOTATION_TEXT)
            .fetch_one(pool)
            .await
            .context("count copies of annotation on reliable mapping")?;
    if reliable_annotation_count != 1 {
        bail!(
            "annotation on reliable mapping must be moved rather than copied; got {reliable_annotation_count} rows"
        );
    }

    let (short_status, short_work_id, source_path): (String, Option<Uuid>, String) =
        sqlx::query_as("SELECT status::text, work_id, file_path FROM books WHERE id = $1")
            .bind(short_book_id)
            .fetch_one(pool)
            .await
            .context("load short version state after resolution")?;
    if short_status != "duplicate" {
        bail!("short version must be marked duplicate, got {short_status:?}");
    }
    let work_id = short_work_id.context("short version must belong to a work after resolution")?;
    let long_work_id: Option<Uuid> = sqlx::query_scalar("SELECT work_id FROM books WHERE id = $1")
        .bind(long_book_id)
        .fetch_one(pool)
        .await
        .context("load long version work after resolution")?;
    if long_work_id != Some(work_id) {
        bail!("both versions must belong to the same work after resolution");
    }
    let primary_book_id: Option<Uuid> =
        sqlx::query_scalar("SELECT primary_book_id FROM book_works WHERE id = $1")
            .bind(work_id)
            .fetch_one(pool)
            .await
            .context("load resolved work primary version")?;
    if primary_book_id != Some(long_book_id) {
        bail!("resolved work primary must be the long B version, got {primary_book_id:?}");
    }

    assert_version_provenance(pool, short_book_id, long_book_id).await?;

    let (kind, status, category, payload, task_book_id, retry_count, max_retries): (
        String,
        String,
        String,
        Value,
        Option<Uuid>,
        i32,
        i32,
    ) = sqlx::query_as(
        r#"SELECT kind::text, status::text, category, payload, book_id,
                  retry_count, max_retries
           FROM tasks WHERE id = $1"#,
    )
    .bind(cleanup_task_id)
    .fetch_one(pool)
    .await
    .context("load retryable dedup index cleanup task")?;
    let cleanup_status_is_live_or_successful = matches!(
        status.as_str(),
        "queued" | "running" | "retrying" | "completed"
    );
    let pair_id_string = pair_id.to_string();
    if !cleanup_status_is_live_or_successful
        || kind != "deduplicate"
        || category != "maintenance"
        || task_book_id != Some(short_book_id)
        || max_retries != 3
        || retry_count > max_retries
        || payload.get("operation").and_then(Value::as_str) != Some("cleanup_secondary_indexes")
        || payload.get("pair_id").and_then(Value::as_str) != Some(pair_id_string.as_str())
        || payload.get("secondary_book_id").and_then(Value::as_str) != Some(SHORT_BOOK_ID)
        || payload.get("primary_book_id").and_then(Value::as_str) != Some(LONG_BOOK_ID)
    {
        bail!(
            "resolution must create a retryable maintenance cleanup task; got kind={kind:?}, status={status:?}, category={category:?}, book_id={task_book_id:?}, retries={retry_count}/{max_retries}, payload={payload}"
        );
    }

    let searchable_policy: Vec<(i32, bool)> = sqlx::query_as(
        r#"SELECT chapter_index, dedup_chapter_is_redundant(id)
           FROM chapters WHERE book_id = $1 ORDER BY chapter_index"#,
    )
    .bind(short_book_id)
    .fetch_all(pool)
    .await
    .context("load pair-scoped redundant chapter search policy")?;
    if searchable_policy.len() != 10
        || searchable_policy[0] != (0, false)
        || searchable_policy[1..]
            .iter()
            .any(|(_, redundant)| !redundant)
    {
        bail!(
            "only exact mapped chapters may be excluded from secondary-version search; got {searchable_policy:?}"
        );
    }

    wait_for_task_status(pool, cleanup_task_id, "completed").await?;
    let cleanup_result: Option<Value> =
        sqlx::query_scalar("SELECT result FROM tasks WHERE id = $1")
            .bind(cleanup_task_id)
            .fetch_one(pool)
            .await
            .context("load completed pair-scoped index cleanup result")?;
    let redundant_indexes = cleanup_result
        .as_ref()
        .and_then(|result| result.get("redundant_chapter_indexes"))
        .and_then(Value::as_array)
        .context("cleanup result must report targeted redundant chapters")?;
    if redundant_indexes.len() != 9
        || redundant_indexes
            .iter()
            .any(|index| index.as_i64() == Some(0))
    {
        bail!("cleanup must retain the approximate chapter index; got {redundant_indexes:?}");
    }

    tokio::fs::metadata(&source_path)
        .await
        .with_context(|| format!("resolution must preserve source file {source_path}"))?;

    Ok(())
}

async fn assert_unchanged_resolution_survives_full_scan(
    client: &Client,
    origin: &str,
    access_token: &str,
    pool: &PgPool,
    library_id: Uuid,
    pair_id: Uuid,
) -> Result<()> {
    let scan = client
        .post(format!("{origin}/api/duplicates/scans"))
        .bearer_auth(access_token)
        .json(&json!({
            "library_id": library_id,
            "include_semantic": false
        }))
        .send()
        .await
        .context("start full scan after resolving duplicate versions")?;
    let scan = response_data(scan).await?;
    let scan_id = scan
        .get("id")
        .and_then(Value::as_str)
        .context("post-resolution full scan response missing id")?;
    let completed = wait_for_completed_scan(client, origin, access_token, scan_id).await?;
    expect_i64(&completed, "pairs_found", 0)?;

    let (stale, resolved, review_status, resolution): (bool, bool, String, Option<String>) =
        sqlx::query_as(
            r#"SELECT stale, resolved, review_status, resolution
               FROM duplicate_pairs WHERE id = $1"#,
        )
        .bind(pair_id)
        .fetch_one(pool)
        .await
        .context("load confirmed pair after unchanged full scan")?;
    if stale || !resolved || review_status != "confirmed" || resolution.as_deref() != Some("keep_b")
    {
        bail!(
            "an unchanged confirmed pair must survive a full scan; got stale={stale}, resolved={resolved}, review_status={review_status:?}, resolution={resolution:?}"
        );
    }

    let visible_pairs = client
        .get(format!(
            "{origin}/api/duplicates?library_id={library_id}&status=confirmed"
        ))
        .bearer_auth(access_token)
        .send()
        .await
        .context("list confirmed pairs after unchanged full scan")?;
    let visible_pairs = response_data(visible_pairs).await?;
    expect_i64(&visible_pairs, "total", 1)?;

    let short_book_id = Uuid::parse_str(SHORT_BOOK_ID)?;
    let searchable_policy: Vec<(i32, bool)> = sqlx::query_as(
        r#"SELECT chapter_index, dedup_chapter_is_redundant(id)
           FROM chapters WHERE book_id = $1 ORDER BY chapter_index"#,
    )
    .bind(short_book_id)
    .fetch_all(pool)
    .await
    .context("load redundant-chapter policy after unchanged full scan")?;
    if searchable_policy.len() != 10
        || searchable_policy[0] != (0, false)
        || searchable_policy[1..]
            .iter()
            .any(|(_, redundant)| !redundant)
    {
        bail!(
            "unchanged full scan must preserve pair-scoped search exclusion; got {searchable_policy:?}"
        );
    }

    Ok(())
}

async fn assert_version_provenance(
    pool: &PgPool,
    short_book_id: Uuid,
    long_book_id: Uuid,
) -> Result<()> {
    let chapter_counts: Vec<(Uuid, i64)> = sqlx::query_as(
        r#"SELECT book_id, COUNT(*)
           FROM chapters
           WHERE book_id = ANY($1)
           GROUP BY book_id
           ORDER BY book_id"#,
    )
    .bind(vec![short_book_id, long_book_id])
    .fetch_all(pool)
    .await
    .context("load concrete-version chapter counts after resolution")?;
    if chapter_counts != vec![(short_book_id, 10), (long_book_id, 15)] {
        bail!(
            "resolution must not move or delete concrete-version chapters; got {chapter_counts:?}"
        );
    }

    let organization_links: Vec<(String, Uuid)> = sqlx::query_as(
        r#"SELECT 'collection', book_id
           FROM collection_books WHERE collection_id = $1
           UNION ALL
           SELECT 'shelf', book_id
           FROM shelf_books WHERE shelf_id = $2
           UNION ALL
           SELECT 'tag', book_id
           FROM book_tags WHERE tag_id = $3
           ORDER BY 1, 2"#,
    )
    .bind(Uuid::parse_str(PROVENANCE_COLLECTION_ID)?)
    .bind(Uuid::parse_str(PROVENANCE_SHELF_ID)?)
    .bind(Uuid::parse_str(PROVENANCE_TAG_ID)?)
    .fetch_all(pool)
    .await
    .context("load shared organization links after resolution")?;
    let expected_organization_links = vec![
        ("collection".to_string(), short_book_id),
        ("collection".to_string(), long_book_id),
        ("shelf".to_string(), short_book_id),
        ("shelf".to_string(), long_book_id),
        ("tag".to_string(), short_book_id),
        ("tag".to_string(), long_book_id),
    ];
    if organization_links != expected_organization_links {
        bail!(
            "collection/shelf/tag links must be copied to the primary while remaining on the source version; got {organization_links:?}"
        );
    }

    let series_links: Vec<(Uuid, f64, Option<String>)> = sqlx::query_as(
        "SELECT book_id, sort_order, volume_label FROM series_books WHERE series_id = $1 ORDER BY book_id",
    )
    .bind(Uuid::parse_str(PROVENANCE_SERIES_ID)?)
    .fetch_all(pool)
    .await
    .context("load edition-specific series provenance after resolution")?;
    if series_links != vec![(short_book_id, 7.0, Some("测试短版".to_string()))] {
        bail!(
            "edition-specific series membership must remain only on the source version; got {series_links:?}"
        );
    }

    let contributor_links: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
        r#"SELECT book_id, person_id, role::text
           FROM book_persons
           WHERE person_id = ANY($1)
           ORDER BY person_id, book_id"#,
    )
    .bind(vec![
        Uuid::parse_str(PROVENANCE_TRANSLATOR_ID)?,
        Uuid::parse_str(PROVENANCE_EDITOR_ID)?,
    ])
    .fetch_all(pool)
    .await
    .context("load edition-specific contributor provenance after resolution")?;
    let expected_contributor_links = vec![
        (
            short_book_id,
            Uuid::parse_str(PROVENANCE_TRANSLATOR_ID)?,
            "translator".to_string(),
        ),
        (
            short_book_id,
            Uuid::parse_str(PROVENANCE_EDITOR_ID)?,
            "editor".to_string(),
        ),
    ];
    if contributor_links != expected_contributor_links {
        bail!(
            "translator/editor provenance must remain only on the concrete source version; got {contributor_links:?}"
        );
    }

    let ratings: Vec<(Uuid, Option<f64>, Option<String>)> = sqlx::query_as(
        r#"SELECT book_id, rating, review
           FROM book_ratings
           WHERE book_id = ANY($1)
           ORDER BY book_id"#,
    )
    .bind(vec![short_book_id, long_book_id])
    .fetch_all(pool)
    .await
    .context("load conflicting version ratings after resolution")?;
    let expected_ratings = vec![
        (short_book_id, Some(6.0), Some("短版独立评分".to_string())),
        (long_book_id, Some(9.0), Some("长版独立评分".to_string())),
    ];
    if ratings != expected_ratings {
        bail!("both version-specific ratings must survive resolution; got {ratings:?}");
    }

    Ok(())
}

async fn seed_containment_fixture(pool: &PgPool, library_root: &Path) -> Result<Uuid> {
    let library_id = Uuid::now_v7();
    let short_book_id = Uuid::parse_str(SHORT_BOOK_ID)?;
    let long_book_id = Uuid::parse_str(LONG_BOOK_ID)?;
    let shared_chapters: Vec<String> = (0..10).map(shared_chapter_content).collect();
    let additional_chapters: Vec<String> = (10..15).map(additional_chapter_content).collect();

    tokio::fs::create_dir_all(library_root)
        .await
        .context("create isolated watched library")?;
    sqlx::query("INSERT INTO libraries (id, name, root_path, book_count) VALUES ($1, $2, $3, 2)")
        .bind(library_id)
        .bind("Dedup E2E Isolated Library")
        .bind(path_string(library_root))
        .execute(pool)
        .await
        .context("insert isolated library")?;

    insert_book(
        pool,
        short_book_id,
        library_id,
        "星海远征（基础版）",
        "/tmp/nova-dedup-e2e-short.txt",
        "dedup-e2e-short-file-hash",
        &shared_chapters,
    )
    .await?;

    let mut extended_chapters = shared_chapters;
    extended_chapters.extend(additional_chapters);
    insert_book(
        pool,
        long_book_id,
        library_id,
        "星海远征（增补版）",
        "/tmp/nova-dedup-e2e-long.txt",
        "dedup-e2e-long-file-hash",
        &extended_chapters,
    )
    .await?;

    Ok(library_id)
}

async fn seed_grouped_boundary_fixture(pool: &PgPool, library_root: &Path) -> Result<Uuid> {
    let library_id = Uuid::now_v7();
    let short_book_id = Uuid::parse_str(SHORT_BOOK_ID)?;
    let long_book_id = Uuid::parse_str(LONG_BOOK_ID)?;
    let shared_chapters: Vec<String> = (0..10).map(shared_chapter_content).collect();
    let merged_shared_chapter = shared_chapters.join("\n\n");
    let mut extended_chapters = vec![merged_shared_chapter];
    extended_chapters.extend((10..15).map(additional_chapter_content));

    tokio::fs::create_dir_all(library_root)
        .await
        .context("create isolated grouped watched library")?;
    sqlx::query("INSERT INTO libraries (id, name, root_path, book_count) VALUES ($1, $2, $3, 2)")
        .bind(library_id)
        .bind("Dedup Grouped E2E Isolated Library")
        .bind(path_string(library_root))
        .execute(pool)
        .await
        .context("insert isolated grouped library")?;

    insert_book(
        pool,
        short_book_id,
        library_id,
        "星海远征（十章分册版）",
        "/tmp/nova-dedup-grouped-e2e-short.txt",
        "dedup-grouped-e2e-short-file-hash",
        &shared_chapters,
    )
    .await?;
    insert_book(
        pool,
        long_book_id,
        library_id,
        "星海远征（合章增补版）",
        "/tmp/nova-dedup-grouped-e2e-long.txt",
        "dedup-grouped-e2e-long-file-hash",
        &extended_chapters,
    )
    .await?;

    Ok(library_id)
}

async fn insert_book(
    pool: &PgPool,
    book_id: Uuid,
    library_id: Uuid,
    title: &str,
    file_path: &str,
    file_hash: &str,
    chapters: &[String],
) -> Result<()> {
    let word_count: i64 = chapters
        .iter()
        .map(|content| content.chars().count() as i64)
        .sum();
    let file_size: i64 = chapters.iter().map(|content| content.len() as i64).sum();

    sqlx::query(
        r#"INSERT INTO books
           (id, library_id, title, author, format, status, file_path, file_hash,
            file_size_bytes, chapter_count, word_count)
           VALUES ($1, $2, $3, '测试作者', 'txt', 'ready', $4, $5, $6, $7, $8)"#,
    )
    .bind(book_id)
    .bind(library_id)
    .bind(title)
    .bind(file_path)
    .bind(file_hash)
    .bind(file_size)
    .bind(i32::try_from(chapters.len()).context("fixture chapter count exceeds i32")?)
    .bind(word_count)
    .execute(pool)
    .await
    .with_context(|| format!("insert fixture book {title}"))?;

    for (index, content) in chapters.iter().enumerate() {
        let index = i32::try_from(index).context("fixture chapter index exceeds i32")?;
        sqlx::query(
            r#"INSERT INTO chapters
               (id, book_id, index, chapter_index, title, content, word_count,
                start_offset, end_offset)
               VALUES ($1, $2, $3, $3, $4, $5, $6, 0, $7)"#,
        )
        .bind(Uuid::now_v7())
        .bind(book_id)
        .bind(index)
        .bind(format!("第{}章", index + 1))
        .bind(content)
        .bind(i32::try_from(content.chars().count()).unwrap_or(i32::MAX))
        .bind(i64::try_from(content.len()).unwrap_or(i64::MAX))
        .execute(pool)
        .await
        .with_context(|| format!("insert chapter {index} for {title}"))?;
    }
    Ok(())
}

fn shared_chapter_content(index: usize) -> String {
    format!(
        "共享章节编号{index:02}。{}结尾标记{index:02}。",
        "远航者记录恒星风暴、舷窗微光与船员之间清晰可辨的对话。".repeat(12)
    )
}

fn additional_chapter_content(index: usize) -> String {
    format!(
        "增补章节编号{index:02}。{}结尾标记{index:02}。",
        "新航线穿过陌生星云，后续事件只存在于内容更多的增补版本中。".repeat(12)
    )
}

async fn wait_for_completed_scan(
    client: &Client,
    origin: &str,
    access_token: &str,
    scan_id: &str,
) -> Result<Value> {
    let mut last_scan = Value::Null;
    for _ in 0..120 {
        let response = client
            .get(format!("{origin}/api/duplicates/scans/{scan_id}"))
            .bearer_auth(access_token)
            .send()
            .await
            .context("poll dedup scan through API")?;
        last_scan = response_data(response).await?;
        match last_scan.get("status").and_then(Value::as_str) {
            Some("completed") => return Ok(last_scan),
            Some("failed") => {
                bail!(
                    "dedup scan failed: {}",
                    last_scan
                        .get("error_message")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown error")
                );
            }
            _ => tokio::time::sleep(Duration::from_millis(250)).await,
        }
    }
    bail!("dedup scan did not complete in 30 seconds; last response: {last_scan}")
}

async fn wait_until_ready(client: &Client, origin: &str, server: &mut Child) -> Result<()> {
    for _ in 0..240 {
        if let Some(status) = server
            .try_wait()
            .context("inspect nova-api child process")?
        {
            bail!("nova-api exited before becoming healthy with status {status}");
        }
        if let Ok(response) = client
            .get(format!("{origin}/api/health"))
            .timeout(Duration::from_millis(500))
            .send()
            .await
        {
            if response.status().is_success() {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    bail!("nova-api did not become healthy in 60 seconds")
}

async fn response_data(response: Response) -> Result<Value> {
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .with_context(|| format!("decode API response body ({status})"))?;
    if !status.is_success() {
        bail!("API returned {status}: {body}");
    }
    body.get("data")
        .cloned()
        .context("API response missing standard data envelope")
}

fn expect_str(value: &Value, field: &str, expected: &str) -> Result<()> {
    let actual = value.get(field).and_then(Value::as_str);
    if actual != Some(expected) {
        bail!("expected {field}={expected:?}, got {actual:?}");
    }
    Ok(())
}

fn expect_nested_str(value: &Value, path: &[&str], expected: &str) -> Result<()> {
    let actual = path
        .iter()
        .try_fold(value, |current, segment| current.get(*segment))
        .and_then(Value::as_str);
    if actual != Some(expected) {
        bail!("expected {}={expected:?}, got {actual:?}", path.join("."));
    }
    Ok(())
}

fn expect_i64(value: &Value, field: &str, expected: i64) -> Result<()> {
    let actual = value.get(field).and_then(Value::as_i64);
    if actual != Some(expected) {
        bail!("expected {field}={expected}, got {actual:?}");
    }
    Ok(())
}

fn expect_approx(value: &Value, field: &str, expected: f64, tolerance: f64) -> Result<()> {
    let actual = value
        .get(field)
        .and_then(Value::as_f64)
        .with_context(|| format!("{field} must be a number"))?;
    if (actual - expected).abs() > tolerance {
        bail!("expected {field}≈{expected}, got {actual}");
    }
    Ok(())
}

fn reserve_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).context("bind temporary TCP port")?;
    Ok(listener.local_addr()?.port())
}

fn spawn_server(database_url: &str, port: u16, temp_root: &Path) -> Result<Child> {
    let data_dir = temp_root.join("data");
    let inbox_dir = temp_root.join("inbox");
    let library_dir = temp_root.join("library");
    std::fs::create_dir_all(&data_dir).context("create isolated data directory")?;

    let child = Command::new(env!("CARGO_BIN_EXE_nova-api"))
        .env("DATABASE_URL", database_url)
        .env("NOVA_HOST", "127.0.0.1")
        .env("NOVA_PORT", port.to_string())
        .env("NOVA_ENV", "development")
        .env("NOVA_WORKER_CONCURRENCY", "1")
        .env("NOVA_DATA_DIR", path_string(&data_dir))
        .env("NOVA_INBOX_DIR", path_string(&inbox_dir))
        .env("NOVA_LIBRARY_DIR", path_string(&library_dir))
        .env("EMBEDDING_ENDPOINT", "")
        .env("RERANKER_ENDPOINT", "")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .context("start nova-api child process")?;
    Ok(child)
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

async fn stop_server(server: &mut Child) {
    let _ = server.start_kill();
    let _ = server.wait().await;
}

async fn create_database(base_url: &str, database_name: &str) -> Result<()> {
    let options = PgConnectOptions::from_str(base_url).context("parse DATABASE_URL")?;
    let mut admin = PgConnection::connect_with(&options.database("postgres"))
        .await
        .context("connect to postgres maintenance database")?;
    admin
        .execute(format!("CREATE DATABASE \"{database_name}\" TEMPLATE template0").as_str())
        .await
        .context("CREATE DATABASE failed")?;
    Ok(())
}

async fn prepare_isolated_schema(database_url: &str) -> Result<()> {
    let pool = PgPool::connect(database_url)
        .await
        .context("connect isolated database for migrations")?;
    match TEST_MIGRATOR.run(&pool).await {
        Ok(()) => {}
        Err(error)
            if error
                .to_string()
                .contains("column \"start_time\" does not exist") =>
        {
            // Historical migration 20260525000005 expects the legacy
            // `reading_sessions.start_time` spelling, while a clean install of
            // 20260524000001 creates `started_at`. Keep this compatibility shim
            // local to the disposable test database; changing an applied
            // migration would invalidate production migration checksums.
            sqlx::query(
                r#"ALTER TABLE reading_sessions
                   ADD COLUMN IF NOT EXISTS start_time TIMESTAMPTZ NOT NULL DEFAULT NOW()"#,
            )
            .execute(&pool)
            .await
            .context("add legacy start_time compatibility column")?;
            TEST_MIGRATOR
                .run(&pool)
                .await
                .context("run migrations after legacy start_time compatibility repair")?;
        }
        Err(error) => return Err(error).context("run migrations in isolated database"),
    }
    pool.close().await;
    Ok(())
}

async fn drop_database(base_url: &str, database_name: &str) -> Result<()> {
    let options = PgConnectOptions::from_str(base_url).context("parse DATABASE_URL")?;
    let mut admin = PgConnection::connect_with(&options.database("postgres"))
        .await
        .context("connect to postgres maintenance database")?;
    admin
        .execute(format!("DROP DATABASE IF EXISTS \"{database_name}\" WITH (FORCE)").as_str())
        .await
        .context("DROP DATABASE failed")?;
    Ok(())
}

fn database_url_for(base_url: &str, database_name: &str) -> Result<String> {
    let scheme_end = base_url
        .find("://")
        .context("DATABASE_URL is missing ://")?
        + 3;
    let path_start = base_url[scheme_end..]
        .find('/')
        .map(|offset| scheme_end + offset)
        .context("DATABASE_URL is missing database path")?;
    let path_end = base_url[path_start..]
        .find('?')
        .map(|offset| path_start + offset)
        .unwrap_or(base_url.len());
    let mut result = String::with_capacity(base_url.len() + database_name.len());
    result.push_str(&base_url[..=path_start]);
    result.push_str(database_name);
    result.push_str(&base_url[path_end..]);
    Ok(result)
}
