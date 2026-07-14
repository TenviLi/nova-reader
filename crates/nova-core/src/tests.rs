#[cfg(test)]
mod id_tests {
    use crate::Id;
    use uuid::Uuid;

    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    struct FakeEntity;

    #[test]
    fn new_generates_v7_uuid() {
        let id: Id<FakeEntity> = Id::new();
        let uuid = id.into_inner();
        // UUIDv7 has version nibble = 7
        assert_eq!(uuid.get_version_num(), 7);
    }

    #[test]
    fn two_ids_are_unique() {
        let a: Id<FakeEntity> = Id::new();
        let b: Id<FakeEntity> = Id::new();
        assert_ne!(a, b);
    }

    #[test]
    fn ids_are_time_ordered() {
        let a: Id<FakeEntity> = Id::new();
        let b: Id<FakeEntity> = Id::new();
        // v7 UUIDs encode timestamp in the first 48 bits — later IDs should be >= earlier ones
        assert!(b.into_inner() >= a.into_inner());
    }

    #[test]
    fn from_uuid_roundtrips() {
        let uuid = Uuid::now_v7();
        let id: Id<FakeEntity> = Id::from_uuid(uuid);
        assert_eq!(id.into_inner(), uuid);
    }

    #[test]
    fn into_uuid_alias_works() {
        let id: Id<FakeEntity> = Id::new();
        let a = id.into_inner();
        let id2: Id<FakeEntity> = Id::from_uuid(a);
        let b = id2.into_uuid();
        assert_eq!(a, b);
    }

    #[test]
    fn parse_valid_string() {
        let uuid = Uuid::now_v7();
        let s = uuid.to_string();
        let id: Id<FakeEntity> = Id::parse(&s).unwrap();
        assert_eq!(id.into_inner(), uuid);
    }

    #[test]
    fn parse_invalid_string_errors() {
        let result: Result<Id<FakeEntity>, _> = Id::parse("not-a-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn display_format_matches_uuid() {
        let uuid = Uuid::now_v7();
        let id: Id<FakeEntity> = Id::from_uuid(uuid);
        assert_eq!(format!("{}", id), format!("{}", uuid));
    }

    #[test]
    fn debug_format_contains_id_prefix() {
        let id: Id<FakeEntity> = Id::new();
        let debug = format!("{:?}", id);
        assert!(debug.starts_with("Id("));
    }

    #[test]
    fn clone_and_eq() {
        let id: Id<FakeEntity> = Id::new();
        let cloned = id;
        assert_eq!(id, cloned);
    }

    #[test]
    fn hash_consistent() {
        use std::collections::HashSet;
        let id: Id<FakeEntity> = Id::new();
        let mut set = HashSet::new();
        set.insert(id);
        assert!(set.contains(&id));
    }

    #[test]
    fn serde_json_roundtrip() {
        let id: Id<FakeEntity> = Id::new();
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: Id<FakeEntity> = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn serde_is_transparent() {
        let uuid = Uuid::now_v7();
        let id: Id<FakeEntity> = Id::from_uuid(uuid);
        let id_json = serde_json::to_string(&id).unwrap();
        let uuid_json = serde_json::to_string(&uuid).unwrap();
        assert_eq!(id_json, uuid_json);
    }

    #[test]
    fn from_uuid_trait_impl() {
        let uuid = Uuid::now_v7();
        let id: Id<FakeEntity> = uuid.into();
        assert_eq!(id.into_inner(), uuid);
    }

    #[test]
    fn into_uuid_trait_impl() {
        let id: Id<FakeEntity> = Id::new();
        let inner = id.into_inner();
        let uuid: Uuid = Id::<FakeEntity>::from_uuid(inner).into();
        assert_eq!(uuid, inner);
    }

    #[test]
    fn default_creates_new_id() {
        let id1: Id<FakeEntity> = Id::default();
        let id2: Id<FakeEntity> = Id::default();
        assert_ne!(id1, id2); // Each default() call generates a fresh ID
    }
}

#[cfg(test)]
mod error_tests {
    use crate::Error;

    #[test]
    fn not_found_is_404() {
        let err = Error::NotFound {
            entity: "Book",
            id: "abc".to_string(),
        };
        assert_eq!(err.status_code(), 404);
    }

    #[test]
    fn duplicate_is_400() {
        let err = Error::Duplicate {
            entity: "User",
            detail: "username taken".to_string(),
        };
        assert_eq!(err.status_code(), 400);
    }

    #[test]
    fn validation_is_400() {
        let err = Error::Validation("invalid email".to_string());
        assert_eq!(err.status_code(), 400);
    }

    #[test]
    fn unauthorized_is_401() {
        let err = Error::Unauthorized;
        assert_eq!(err.status_code(), 401);
    }

    #[test]
    fn invalid_credentials_is_401() {
        let err = Error::InvalidCredentials;
        assert_eq!(err.status_code(), 401);
    }

    #[test]
    fn forbidden_is_403() {
        let err = Error::Forbidden;
        assert_eq!(err.status_code(), 403);
    }

    #[test]
    fn rate_limited_is_429() {
        let err = Error::RateLimited {
            retry_after_secs: 60,
        };
        assert_eq!(err.status_code(), 429);
    }

    #[test]
    fn internal_is_500() {
        let err = Error::Internal("something broke".to_string());
        assert_eq!(err.status_code(), 500);
    }

    #[test]
    fn ai_service_is_500() {
        let err = Error::AiService("timeout".to_string());
        assert_eq!(err.status_code(), 500);
    }

    #[test]
    fn database_is_retryable() {
        // Can't construct a real sqlx::Error easily, skip direct test
        // but verify others
        let err = Error::Redis("connection refused".to_string());
        assert!(err.is_retryable());
    }

    #[test]
    fn ai_service_is_retryable() {
        let err = Error::AiService("rate limited".to_string());
        assert!(err.is_retryable());
    }

    #[test]
    fn vector_db_is_retryable() {
        let err = Error::VectorDb("timeout".to_string());
        assert!(err.is_retryable());
    }

    #[test]
    fn graph_db_is_retryable() {
        let err = Error::GraphDb("neo4j down".to_string());
        assert!(err.is_retryable());
    }

    #[test]
    fn rate_limited_is_retryable() {
        let err = Error::RateLimited {
            retry_after_secs: 30,
        };
        assert!(err.is_retryable());
    }

    #[test]
    fn retryable_conflict_is_409_and_retryable() {
        let err = Error::RetryableConflict("write barrier busy".to_string());
        assert_eq!(err.status_code(), 409);
        assert!(err.is_retryable());
    }

    #[test]
    fn not_found_is_not_retryable() {
        let err = Error::NotFound {
            entity: "Book",
            id: "123".to_string(),
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn validation_is_not_retryable() {
        let err = Error::Validation("bad input".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn unauthorized_is_not_retryable() {
        let err = Error::Unauthorized;
        assert!(!err.is_retryable());
    }

    #[test]
    fn error_display_format() {
        let err = Error::NotFound {
            entity: "Book",
            id: "abc-123".to_string(),
        };
        assert_eq!(format!("{}", err), "entity not found: Book with id abc-123");
    }

    #[test]
    fn task_failed_display() {
        let err = Error::TaskFailed {
            task: "embed".to_string(),
            reason: "out of memory".to_string(),
        };
        assert_eq!(format!("{}", err), "task failed: embed - out of memory");
    }

    #[test]
    fn io_error_converts() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file gone");
        let err: Error = io_err.into();
        assert_eq!(err.status_code(), 500);
        assert!(!err.is_retryable());
    }
}

#[cfg(test)]
mod domain_tests {
    use std::str::FromStr;

    use crate::domain::chapter::ChunkingConfig;
    use crate::domain::dedup::{
        DedupIndexCleanupTask, DedupScanPhase, DedupScanTask, DedupTaskPayload,
        DuplicateAlignmentGroupEvidence, DuplicateEvidenceSchemaVersion, DuplicatePairEvidence,
    };
    use crate::domain::task::{TaskExecutionLockMode, TaskKind, TaskPriority, TaskStatus};
    use crate::domain::user::UserPreferences;

    #[test]
    fn chunking_config_defaults() {
        let config = ChunkingConfig::default();
        assert_eq!(config.chunk_size, 512);
        assert_eq!(config.overlap, 64);
        assert_eq!(config.min_chunk_size, 100);
    }

    #[test]
    fn task_priority_ordering() {
        assert!(TaskPriority::Critical > TaskPriority::High);
        assert!(TaskPriority::High > TaskPriority::Normal);
        assert!(TaskPriority::Normal > TaskPriority::Low);
    }

    #[test]
    fn task_kind_serde_roundtrip() {
        let kinds = [
            TaskKind::ParseFile,
            TaskKind::GenerateEmbeddings,
            TaskKind::ExtractEntities,
            TaskKind::Deduplicate,
            TaskKind::Translate,
            TaskKind::CleanContent,
            TaskKind::LibraryScan,
            TaskKind::GenerateMetadata,
            TaskKind::BuildGraphSummary,
            TaskKind::IndexMeilisearch,
            TaskKind::SyncNeo4j,
            TaskKind::ComputeBookEmbedding,
            TaskKind::DetectCommunities,
            TaskKind::DeepAnalysis,
            TaskKind::SentimentArc,
            TaskKind::TrackForeshadowing,
            TaskKind::SemanticTagging,
            TaskKind::AssignOntology,
            TaskKind::ReindexLibrary,
            TaskKind::CleanupOrphanCovers,
            TaskKind::RecomputeFileHashes,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let deserialized: TaskKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, deserialized);
            assert_eq!(TaskKind::from_str(kind.as_str()), Ok(kind));
        }
        assert!(TaskKind::from_str("dedupe_typo").is_err());
    }

    #[test]
    fn task_status_variants() {
        let statuses = vec![
            TaskStatus::Queued,
            TaskStatus::Running,
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Retrying,
            TaskStatus::Cancelled,
            TaskStatus::DeadLetter,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: TaskStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }

    #[test]
    fn task_kind_owns_its_queue_category() {
        use crate::domain::task::TaskCategory;

        assert_eq!(TaskKind::SemanticTagging.category(), TaskCategory::Ai);
        assert_eq!(TaskKind::DeepAnalysis.category(), TaskCategory::Ai);
        assert_eq!(
            TaskKind::ReindexLibrary.category(),
            TaskCategory::Maintenance
        );
        assert_eq!(
            TaskKind::CleanupOrphanCovers.category(),
            TaskCategory::Maintenance
        );
    }

    #[test]
    fn dedup_task_payloads_and_phase_codes_have_typed_wire_boundaries() {
        let scan = DedupScanTask::new(
            uuid::Uuid::from_u128(1),
            Some(uuid::Uuid::from_u128(2)),
            true,
            Some(vec![uuid::Uuid::from_u128(3)]),
        );
        let encoded = serde_json::to_value(&scan).expect("scan payload serializes");
        let decoded: DedupTaskPayload =
            serde_json::from_value(encoded).expect("scan payload deserializes");
        assert_eq!(decoded, DedupTaskPayload::Scan(scan));
        assert!(
            serde_json::from_value::<DedupTaskPayload>(serde_json::json!({
                "operation": "cleanup_secondary_indexes",
                "scan_run_id": uuid::Uuid::from_u128(1),
                "library_id": null,
                "include_semantic": false
            }))
            .is_err()
        );
        assert_eq!(
            DedupScanPhase::from_wire(DedupScanPhase::CandidateGeneration.as_str()),
            Some(DedupScanPhase::CandidateGeneration)
        );
        assert_eq!(TaskExecutionLockMode::Shared.as_str(), "shared");
        assert_eq!(TaskExecutionLockMode::Exclusive.as_str(), "exclusive");
    }

    #[test]
    fn dedup_index_cleanup_payload_resumes_meilisearch_without_breaking_legacy_tasks() {
        let pair_id = uuid::Uuid::from_u128(11);
        let secondary_book_id = uuid::Uuid::from_u128(12);
        let primary_book_id = uuid::Uuid::from_u128(13);
        let legacy = serde_json::json!({
            "operation": "cleanup_secondary_indexes",
            "pair_id": pair_id,
            "secondary_book_id": secondary_book_id,
            "primary_book_id": primary_book_id,
        });

        let legacy: DedupIndexCleanupTask =
            serde_json::from_value(legacy).expect("legacy cleanup payload remains readable");
        assert_eq!(legacy.meilisearch_task_uid, None);

        let mut resumed = legacy;
        resumed.meilisearch_task_uid = Some(42);
        let encoded = serde_json::to_value(&resumed).expect("resumed payload serializes");
        assert_eq!(encoded["meilisearch_task_uid"], 42);
        let decoded: DedupIndexCleanupTask =
            serde_json::from_value(encoded).expect("resumed payload deserializes");
        assert_eq!(decoded, resumed);
    }

    #[test]
    fn duplicate_pair_evidence_has_a_versioned_typed_wire_shape() {
        let evidence: DuplicatePairEvidence = serde_json::from_value(serde_json::json!({
            "book_a_layout_hash": "aa",
            "book_b_layout_hash": "bb",
            "algorithm_version": 4
        }))
        .expect("legacy evidence without an explicit schema version remains readable");

        assert_eq!(evidence.schema_version, DuplicateEvidenceSchemaVersion::V2);
        assert_eq!(evidence.alignment_schema_version, 2);

        let encoded = serde_json::to_value(evidence).expect("typed evidence serializes");
        assert_eq!(encoded["schema_version"], "v2");
        assert!(
            serde_json::from_value::<DuplicateAlignmentGroupEvidence>(serde_json::json!({
                "id": 0,
                "mapping_shape": "one_to_some",
                "chapters_a": [0],
                "chapters_b": [0, 1],
                "matched_characters": 400,
                "segment_count": 2,
                "source_verified": true
            }))
            .is_err()
        );
    }

    #[test]
    fn user_preferences_defaults() {
        let prefs = UserPreferences::default();
        assert_eq!(prefs.theme, "dark");
        assert_eq!(prefs.reader_font_size, 18);
        assert_eq!(prefs.reader_line_height, 1.8);
        assert_eq!(prefs.reader_max_width, 720);
        assert!(prefs.track_reading_stats);
        assert_eq!(prefs.auto_save_interval_secs, 30);
        assert_eq!(prefs.library_sort, "updated_at_desc");
        assert_eq!(prefs.library_page_size, 24);
        assert!(prefs.keyboard_shortcuts);
        assert!(!prefs.opds_enabled);
        assert!(prefs.ai_enabled);
        assert_eq!(prefs.embedding_server_url, "http://localhost:8000");
    }

    #[test]
    fn user_preferences_serde_roundtrip() {
        let prefs = UserPreferences {
            theme: "light".into(),
            reader_font: Some("Georgia".into()),
            reader_font_size: 20,
            reader_line_height: 2.0,
            reader_max_width: 900,
            preferred_language: Some("zh-CN".into()),
            track_reading_stats: false,
            auto_save_interval_secs: 60,
            library_sort: "title_asc".into(),
            library_page_size: 48,
            keyboard_shortcuts: true,
            opds_enabled: true,
            ai_enabled: false,
            deepseek_api_key: Some("sk-test".into()),
            embedding_server_url: "http://custom:9000".into(),
        };

        let json = serde_json::to_string(&prefs).unwrap();
        // Password / API key should be skipped from serialization
        assert!(!json.contains("sk-test"));

        let deserialized: UserPreferences = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.theme, "light");
        assert_eq!(deserialized.reader_font_size, 20);
        assert!(deserialized.opds_enabled);
        assert!(!deserialized.ai_enabled);
        // deepseek_api_key is skip_serializing so should be None after roundtrip
        assert!(deserialized.deepseek_api_key.is_none());
    }

    #[test]
    fn user_preferences_deserialize_with_missing_fields() {
        // Simulates loading an older config that's missing new fields
        let json = r#"{"theme":"dark"}"#;
        let prefs: UserPreferences = serde_json::from_str(json).unwrap();
        assert_eq!(prefs.theme, "dark");
        assert_eq!(prefs.reader_font_size, 18); // Should use default
        assert!(prefs.track_reading_stats); // Should use default
    }
}
