use sqlx::{migrate::Migration, PgPool};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

pub async fn run_database_migrations(db: &PgPool) -> anyhow::Result<()> {
    repair_legacy_sqlx_migrations(db).await?;
    MIGRATOR.run(db).await?;
    Ok(())
}

async fn repair_legacy_sqlx_migrations(db: &PgPool) -> anyhow::Result<()> {
    if !sqlx_migrations_table_exists(db).await? {
        return Ok(());
    }

    let legacy_rows: Vec<(i64, Vec<u8>)> = sqlx::query_as(
        r#"
        SELECT version, checksum
        FROM _sqlx_migrations
        WHERE success = true
          AND (
            version BETWEEN 20000101 AND 20991231
            OR version BETWEEN 20000101001 AND 20991231999
          )
        ORDER BY version
        "#,
    )
    .fetch_all(db)
    .await?;

    if legacy_rows.is_empty() {
        return Ok(());
    }

    let signatures = migration_signatures(&MIGRATOR.migrations);
    let mut inserted = 0u64;

    for (legacy_version, checksum) in &legacy_rows {
        if let Some(migration) =
            find_legacy_checksum_match(*legacy_version, checksum.as_slice(), &signatures)
        {
            inserted += insert_repaired_migration(db, migration).await?;
        }
    }

    for migration in &signatures {
        if schema_marker_satisfied(db, migration.version).await? {
            inserted += insert_repaired_migration(db, migration).await?;
        }
    }

    let mut deleted = 0u64;
    for (legacy_version, _) in legacy_rows {
        if has_current_migration_for_legacy_date(db, legacy_version).await? {
            deleted += sqlx::query("DELETE FROM _sqlx_migrations WHERE version = $1")
                .bind(legacy_version)
                .execute(db)
                .await?
                .rows_affected();
        }
    }

    if inserted > 0 || deleted > 0 {
        tracing::info!(
            inserted,
            deleted,
            "repaired legacy SQLx migration metadata after timestamped migration rename"
        );
    }

    Ok(())
}

async fn sqlx_migrations_table_exists(db: &PgPool) -> anyhow::Result<bool> {
    let exists = sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations') IS NOT NULL")
        .fetch_one(db)
        .await?;
    Ok(exists)
}

async fn insert_repaired_migration(
    db: &PgPool,
    migration: &MigrationSignature<'_>,
) -> anyhow::Result<u64> {
    let result = sqlx::query(
        r#"
        INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
        VALUES ($1, $2, true, $3, 0)
        ON CONFLICT (version) DO NOTHING
        "#,
    )
    .bind(migration.version)
    .bind(migration.description)
    .bind(migration.checksum)
    .execute(db)
    .await?;

    Ok(result.rows_affected())
}

async fn has_current_migration_for_legacy_date(
    db: &PgPool,
    legacy_version: i64,
) -> anyhow::Result<bool> {
    if let Some(current_version) = current_version_for_legacy_sequence(legacy_version) {
        let exists = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM _sqlx_migrations
                WHERE success = true
                  AND version = $1
            )
            "#,
        )
        .bind(current_version)
        .fetch_one(db)
        .await?;

        return Ok(exists);
    }

    let legacy_date = legacy_date_for_migration_version(legacy_version);
    let start = legacy_date * 1_000_000;
    let end = (legacy_date + 1) * 1_000_000;
    let exists = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM _sqlx_migrations
            WHERE success = true
              AND version >= $1
              AND version < $2
        )
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_one(db)
    .await?;

    Ok(exists)
}

#[derive(Debug)]
struct MigrationSignature<'a> {
    version: i64,
    description: &'a str,
    checksum: &'a [u8],
}

fn migration_signatures(migrations: &[Migration]) -> Vec<MigrationSignature<'_>> {
    migrations
        .iter()
        .filter(|migration| migration.migration_type.is_up_migration())
        .map(|migration| MigrationSignature {
            version: migration.version,
            description: migration.description.as_ref(),
            checksum: migration.checksum.as_ref(),
        })
        .collect()
}

fn is_legacy_sqlx_date_version(version: i64) -> bool {
    (20_000_101..=20_991_231).contains(&version)
        || (20_000_101_001..=20_991_231_999).contains(&version)
}

fn current_version_for_legacy_sequence(version: i64) -> Option<i64> {
    if !(20_000_101_001..=20_991_231_999).contains(&version) {
        return None;
    }

    let date = version / 1_000;
    let sequence = version % 1_000;
    Some(date * 1_000_000 + sequence)
}

fn legacy_date_for_migration_version(version: i64) -> i64 {
    if version <= 20_991_231 {
        version
    } else if version <= 20_991_231_999 {
        version / 1_000
    } else {
        version / 1_000_000
    }
}

fn find_legacy_checksum_match<'a>(
    legacy_version: i64,
    legacy_checksum: &[u8],
    migrations: &'a [MigrationSignature<'a>],
) -> Option<&'a MigrationSignature<'a>> {
    if !is_legacy_sqlx_date_version(legacy_version) {
        return None;
    }

    let legacy_date = legacy_date_for_migration_version(legacy_version);
    migrations.iter().find(|migration| {
        legacy_date_for_migration_version(migration.version) == legacy_date
            && migration.checksum == legacy_checksum
    })
}

#[derive(Clone, Copy)]
enum SchemaRequirement {
    Table(&'static str),
    Column {
        table: &'static str,
        column: &'static str,
    },
    NullableColumn {
        table: &'static str,
        column: &'static str,
    },
    Type(&'static str),
    EnumValue {
        type_name: &'static str,
        value: &'static str,
    },
    Index(&'static str),
}

async fn schema_marker_satisfied(db: &PgPool, version: i64) -> anyhow::Result<bool> {
    let Some(requirements) = schema_requirements_for_migration(version) else {
        return Ok(false);
    };

    for requirement in requirements {
        if !schema_requirement_satisfied(db, *requirement).await? {
            return Ok(false);
        }
    }

    Ok(true)
}

fn schema_requirements_for_migration(version: i64) -> Option<&'static [SchemaRequirement]> {
    use SchemaRequirement::*;

    match version {
        20260524000001 => Some(&[
            Table("books"),
            Table("tasks"),
            Table("duplicate_pairs"),
            Table("system_config"),
        ]),
        20260524000002 => Some(&[
            Table("characters"),
            Table("file_signatures"),
            Column {
                table: "libraries",
                column: "scan_status",
            },
        ]),
        20260524000003 => Some(&[
            Table("refresh_tokens"),
            Table("book_ratings"),
            Table("reading_goals"),
            Column {
                table: "entities",
                column: "profile",
            },
        ]),
        20260525000004 => Some(&[
            Column {
                table: "persons",
                column: "original_name",
            },
            Column {
                table: "bookmarks",
                column: "note",
            },
        ]),
        20260525000005 => Some(&[Table("user_settings"), Table("reading_activities")]),
        20260525000023 => Some(&[
            Table("task_dependencies"),
            Column {
                table: "tasks",
                column: "category",
            },
        ]),
        20260525000024 => Some(&[Table("chapter_summaries"), Table("macro_analysis")]),
        20260525000025 => Some(&[Table("tag_profiles"), Table("vibe_bookmarks")]),
        20260525000026 => Some(&[Table("trope_nodes"), Table("ontology_events")]),
        20260526000006 => Some(&[Column {
            table: "libraries",
            column: "compute_hashes",
        }]),
        20260526000007 => Some(&[
            Table("ai_usage_logs"),
            Table("entity_profiles"),
            Table("setting_profiles"),
        ]),
        20260527000008 => Some(&[
            Table("collection_shares"),
            Table("smart_shelves"),
            Table("annotation_shares"),
            Column {
                table: "books",
                column: "custom_fields",
            },
        ]),
        20260528000009 => Some(&[
            Type("reading_status"),
            Column {
                table: "books",
                column: "reading_status",
            },
        ]),
        20260528000010 => Some(&[
            Type("user_role"),
            Column {
                table: "users",
                column: "role",
            },
            Table("library_permissions"),
        ]),
        20260528000011 => Some(&[Table("webhooks"), Table("webhook_deliveries")]),
        20260528000012 => Some(&[Table("notification_channels")]),
        20260528000013 => Some(&[
            Table("book_clubs"),
            Table("annotation_replies"),
            Column {
                table: "annotations",
                column: "visibility",
            },
        ]),
        20260528000014 => Some(&[Table("book_editions"), Table("edition_diffs")]),
        20260528000015 => Some(&[
            Type("import_source_type"),
            Type("import_status"),
            Table("import_sources"),
            Table("import_logs"),
        ]),
        20260528000016 => Some(&[Table("plugins"), Table("plugin_executions")]),
        20260528000017 => Some(&[
            Table("friendships"),
            Table("user_activities"),
            Table("reading_challenges"),
        ]),
        20260528000018 => Some(&[
            NullableColumn {
                table: "annotations",
                column: "chapter_id",
            },
            NullableColumn {
                table: "bookmarks",
                column: "chapter_id",
            },
        ]),
        20260528000019 => Some(&[
            Column {
                table: "reading_progress",
                column: "chapter_index",
            },
            Column {
                table: "entity_mentions",
                column: "book_id",
            },
            Column {
                table: "collections",
                column: "book_count",
            },
        ]),
        20260529000020 => Some(&[Column {
            table: "libraries",
            column: "is_default",
        }]),
        20260529000021 => Some(&[
            Table("ai_conversations"),
            Table("ai_conversation_messages"),
            Table("feature_flags"),
        ]),
        20260529000022 => Some(&[Index("idx_books_library_status")]),
        20260530000027 => Some(&[EnumValue {
            type_name: "book_format",
            value: "doc",
        }]),
        20260531000028 => Some(&[Table("notifications")]),
        20260531000029 => Some(&[Column {
            table: "libraries",
            column: "features",
        }]),
        20260531000030 => Some(&[Table("recommendation_feedback")]),
        20260531000031 => Some(&[Table("user_groups"), Table("library_group_permissions")]),
        20260531000032 => Some(&[Table("permission_templates")]),
        20260531000033 => Some(&[
            EnumValue {
                type_name: "task_kind",
                value: "deep_analysis",
            },
            EnumValue {
                type_name: "task_kind",
                value: "reindex_library",
            },
        ]),
        20260531000034 => Some(&[
            Index("idx_chapters_book_index"),
            Index("idx_annotations_user_book"),
        ]),
        20260531000035 => Some(&[
            Column {
                table: "reading_progress",
                column: "user_id",
            },
            Column {
                table: "bookmarks",
                column: "user_id",
            },
        ]),
        20260531000036 => Some(&[
            Column {
                table: "collections",
                column: "owner_id",
            },
            Column {
                table: "shelves",
                column: "owner_id",
            },
            Column {
                table: "smart_shelves",
                column: "owner_id",
            },
            Index("idx_collections_owner"),
            Index("idx_shelves_owner"),
            Index("idx_smart_shelves_owner"),
        ]),
        20260713000037 => Some(&[
            Table("book_fingerprints"),
            Table("chapter_fingerprints"),
            Table("passage_fingerprints"),
            Table("dedup_scan_runs"),
            Table("duplicate_chapter_matches"),
            Table("book_works"),
            Column {
                table: "books",
                column: "work_id",
            },
        ]),
        _ => None,
    }
}

async fn schema_requirement_satisfied(
    db: &PgPool,
    requirement: SchemaRequirement,
) -> anyhow::Result<bool> {
    let exists = match requirement {
        SchemaRequirement::Table(table) => {
            sqlx::query_scalar(
                r#"
            SELECT EXISTS(
                SELECT 1
                FROM information_schema.tables
                WHERE table_schema = 'public'
                  AND table_name = $1
            )
            "#,
            )
            .bind(table)
            .fetch_one(db)
            .await?
        }
        SchemaRequirement::Column { table, column } => {
            sqlx::query_scalar(
                r#"
            SELECT EXISTS(
                SELECT 1
                FROM information_schema.columns
                WHERE table_schema = 'public'
                  AND table_name = $1
                  AND column_name = $2
            )
            "#,
            )
            .bind(table)
            .bind(column)
            .fetch_one(db)
            .await?
        }
        SchemaRequirement::NullableColumn { table, column } => {
            sqlx::query_scalar(
                r#"
            SELECT EXISTS(
                SELECT 1
                FROM information_schema.columns
                WHERE table_schema = 'public'
                  AND table_name = $1
                  AND column_name = $2
                  AND is_nullable = 'YES'
            )
            "#,
            )
            .bind(table)
            .bind(column)
            .fetch_one(db)
            .await?
        }
        SchemaRequirement::Type(type_name) => {
            sqlx::query_scalar(
                r#"
            SELECT EXISTS(
                SELECT 1
                FROM pg_type
                WHERE typname = $1
            )
            "#,
            )
            .bind(type_name)
            .fetch_one(db)
            .await?
        }
        SchemaRequirement::EnumValue { type_name, value } => {
            sqlx::query_scalar(
                r#"
            SELECT EXISTS(
                SELECT 1
                FROM pg_type t
                JOIN pg_enum e ON e.enumtypid = t.oid
                WHERE t.typname = $1
                  AND e.enumlabel = $2
            )
            "#,
            )
            .bind(type_name)
            .bind(value)
            .fetch_one(db)
            .await?
        }
        SchemaRequirement::Index(index) => {
            sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
                .bind(index)
                .fetch_one(db)
                .await?
        }
    };

    Ok(exists)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_legacy_sqlx_date_versions() {
        assert!(is_legacy_sqlx_date_version(20260524));
        assert!(is_legacy_sqlx_date_version(20260531));
        assert!(is_legacy_sqlx_date_version(20260524001));
        assert!(is_legacy_sqlx_date_version(20260531036));

        assert!(!is_legacy_sqlx_date_version(20260524000001));
        assert!(!is_legacy_sqlx_date_version(60524));
    }

    #[test]
    fn derives_legacy_date_from_full_timestamp_version() {
        assert_eq!(legacy_date_for_migration_version(20260524), 20260524);
        assert_eq!(legacy_date_for_migration_version(20260524001), 20260524);
        assert_eq!(legacy_date_for_migration_version(20260524000001), 20260524);
        assert_eq!(legacy_date_for_migration_version(20260531000036), 20260531);
    }

    #[test]
    fn maps_legacy_date_plus_sequence_to_current_version() {
        assert_eq!(
            current_version_for_legacy_sequence(20260524001),
            Some(20260524000001)
        );
        assert_eq!(
            current_version_for_legacy_sequence(20260531036),
            Some(20260531000036)
        );
        assert_eq!(current_version_for_legacy_sequence(20260524), None);
        assert_eq!(current_version_for_legacy_sequence(20260524000001), None);
    }

    #[test]
    fn legacy_checksum_match_requires_same_date_and_checksum() {
        let migrations = vec![
            MigrationSignature {
                version: 20260524000001,
                description: "initial_schema",
                checksum: b"initial".as_slice(),
            },
            MigrationSignature {
                version: 20260525000004,
                description: "schema_refinements",
                checksum: b"same-bytes".as_slice(),
            },
        ];

        let matched = find_legacy_checksum_match(20260524, b"initial", &migrations)
            .expect("legacy row should map to renamed migration with same date and checksum");
        assert_eq!(matched.version, 20260524000001);

        let matched = find_legacy_checksum_match(20260524001, b"initial", &migrations)
            .expect("legacy date-plus-sequence row should map to the renamed migration");
        assert_eq!(matched.version, 20260524000001);

        assert!(find_legacy_checksum_match(20260524, b"same-bytes", &migrations).is_none());
        assert!(find_legacy_checksum_match(20260524, b"missing", &migrations).is_none());
    }

    #[test]
    fn owner_migration_marker_requires_owner_indexes() {
        let requirements = schema_requirements_for_migration(20260531000036)
            .expect("owner migration should have schema requirements");

        assert!(requirements.iter().any(|requirement| matches!(
            requirement,
            SchemaRequirement::Index("idx_collections_owner")
        )));
        assert!(requirements.iter().any(|requirement| matches!(
            requirement,
            SchemaRequirement::Index("idx_shelves_owner")
        )));
        assert!(requirements.iter().any(|requirement| matches!(
            requirement,
            SchemaRequirement::Index("idx_smart_shelves_owner")
        )));
    }

    #[test]
    fn owner_migration_backfills_single_user_legacy_containers() {
        let sql = include_str!("../../../migrations/20260531000036_collection_shelf_owners.sql");

        assert!(sql.contains("singleton_owner"));
        assert!(sql.contains("SELECT COUNT(*) FROM users) = 1"));
        assert!(sql.contains("UPDATE collections"));
        assert!(sql.contains("UPDATE shelves"));
        assert!(sql.contains("UPDATE smart_shelves"));
    }
}
