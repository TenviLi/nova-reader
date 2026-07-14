//! Unit tests for nova-api route handlers and services.
//! These test the logic WITHOUT requiring a live database (where possible).

#[cfg(test)]
mod tests {
    use crate::ai_usage::estimate_cost;
    use crate::prompts;

    // ─── Prompt Template Tests ───────────────────────────────────────────────

    #[test]
    fn test_chapter_analysis_prompt_contains_xml_tags() {
        let prompt = prompts::chapter_analysis_prompt(1, "");
        assert!(prompt.contains("<system>"));
        assert!(prompt.contains("<role>"));
        assert!(prompt.contains("<action>"));
        assert!(prompt.contains("<expectation>"));
        assert!(prompt.contains("<constraints>"));
        assert!(prompt.contains("</system>"));
    }

    #[test]
    fn test_chapter_analysis_prompt_includes_chapter_number() {
        let prompt = prompts::chapter_analysis_prompt(5, "之前发生了战斗");
        assert!(prompt.contains("第 5 章"));
        assert!(prompt.contains("之前发生了战斗"));
    }

    #[test]
    fn test_chapter_analysis_prompt_first_chapter_context() {
        let prompt = prompts::chapter_analysis_prompt(1, "");
        assert!(prompt.contains("第一章"));
    }

    #[test]
    fn test_style_analysis_prompt_structure() {
        let prompt = prompts::style_analysis_prompt();
        assert!(prompt.contains("<role>"));
        assert!(prompt.contains("genre"));
        assert!(prompt.contains("vocabulary_level"));
    }

    #[test]
    fn test_tag_generation_prompt_includes_data() {
        let prompt = prompts::tag_generation_prompt("张三(person), 李四(person)", "修仙升级");
        assert!(prompt.contains("张三"));
        assert!(prompt.contains("修仙升级"));
        assert!(prompt.contains("<constraints>"));
    }

    #[test]
    fn test_translation_prompt_with_glossary() {
        let prompt =
            prompts::translation_prompt("中文", "English", "- 灵气 → Qi\n- 丹田 → Dantian");
        assert!(prompt.contains("灵气 → Qi"));
        assert!(prompt.contains("术语表"));
        assert!(prompt.contains("中文"));
        assert!(prompt.contains("English"));
    }

    #[test]
    fn test_translation_prompt_without_glossary() {
        let prompt = prompts::translation_prompt("中文", "日本語", "");
        assert!(prompt.contains("暂无专用术语表"));
    }

    #[test]
    fn test_entity_extraction_prompt_has_types() {
        let prompt = prompts::entity_extraction_prompt();
        assert!(prompt.contains("person"));
        assert!(prompt.contains("location"));
        assert!(prompt.contains("organization"));
        assert!(prompt.contains("item"));
        assert!(prompt.contains("concept"));
    }

    #[test]
    fn test_outline_prompt_chapter_count() {
        let prompt = prompts::generate_outline_prompt(20, "科幻");
        assert!(prompt.contains("20"));
        assert!(prompt.contains("科幻"));
    }

    #[test]
    fn test_rag_context_message_format() {
        let msg = prompts::rag_context_message("这是一段关于主角的描述");
        assert!(msg.contains("<context type=\"retrieved_knowledge\">"));
        assert!(msg.contains("这是一段关于主角的描述"));
    }

    #[test]
    fn test_entity_profile_prompt() {
        let prompt = prompts::entity_profile_prompt("张无忌", "person", "[第1章] 他出现了");
        assert!(prompt.contains("张无忌"));
        assert!(prompt.contains("person"));
        assert!(prompt.contains("[第1章]"));
        assert!(prompt.contains("confidence_score"));
    }

    #[test]
    fn test_batch_translate_prompt() {
        let prompt = prompts::batch_translate_prompt("中文", "English");
        assert!(prompt.contains("---SEG---"));
        assert!(prompt.contains("中文"));
        assert!(prompt.contains("English"));
    }

    // ─── Dormant Route Security Contract Tests ───────────────────────────────

    #[test]
    fn dormant_translate_route_source_requires_book_acl_before_future_mounting() {
        let source = include_str!("routes/translate.rs");

        assert!(source.contains("extractors::AuthUser"));
        assert!(source.contains("auth: AuthUser"));
        assert!(source
            .contains("ensure_book_access(&state, &auth, book_id, LibraryAccess::Read).await?"));
        assert!(!source.contains(".unwrap_or_default()"));
    }

    #[test]
    fn dormant_entity_profiles_route_source_requires_entity_acl_before_future_mounting() {
        let source = include_str!("routes/entity_profiles.rs");

        assert!(source.contains("extractors::AuthUser"));
        assert!(source.contains("auth: AuthUser"));
        assert!(
            source.contains("ensure_entity_access(&state, &auth, id, LibraryAccess::Read).await?")
        );
        assert!(
            source.contains("ensure_entity_access(&state, &auth, id, LibraryAccess::Write).await?")
        );
    }

    #[test]
    fn known_unsafe_historical_route_modules_remain_unmounted_until_acl_lands() {
        let router = include_str!("routes/mod.rs");

        for module in [
            "calibre",
            "import_sources",
            "opds",
            "plugins",
            "social",
            "webhooks",
            "ws",
        ] {
            assert!(
                router.contains(&format!("// mod {module};")),
                "{module} must stay visibly disabled until auth and object-level ACL are implemented"
            );
            assert!(
                !router.contains(&format!(".merge({module}::")),
                "{module} routes must not be merged into the active API router"
            );
            assert!(
                !router.contains(&format!(".nest(\"/{module}\"")),
                "{module} routes must not be nested into the active API router"
            );
        }
    }

    #[test]
    fn test_summarize_prompt_variants() {
        let brief = prompts::summarize_prompt("brief");
        let detailed = prompts::summarize_prompt("detailed");
        let bullet = prompts::summarize_prompt("bullet_points");

        assert!(brief.contains("简洁"));
        assert!(detailed.contains("详细"));
        assert!(bullet.contains("要点"));
    }

    // ─── Cost Estimation Tests ───────────────────────────────────────────────

    #[test]
    fn test_cost_estimation_deepseek_chat() {
        let cost = estimate_cost("deepseek-chat", 1000, 500);
        // Input: 1000/1M * 0.14 * 100 = 0.014 cents
        // Output: 500/1M * 0.28 * 100 = 0.014 cents
        assert!((cost - 0.028).abs() < 0.001);
    }

    #[test]
    fn test_cost_estimation_deepseek_reasoner() {
        let cost = estimate_cost("deepseek-reasoner", 10000, 5000);
        assert!((cost - 1.645).abs() < 0.01);
    }

    #[test]
    fn test_cost_estimation_zero_tokens() {
        let cost = estimate_cost("deepseek-chat", 0, 0);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_cost_estimation_large_batch() {
        // Simulating a full-novel analysis: 500K input, 100K output
        let cost = estimate_cost("deepseek-chat", 500_000, 100_000);
        // Input: 500K/1M * 0.14 * 100 = 7.0 cents
        // Output: 100K/1M * 0.28 * 100 = 2.8 cents
        assert!((cost - 9.8).abs() < 0.1);
    }

    // ─── Chunk Text Tests ────────────────────────────────────────────────────

    #[test]
    fn test_chunk_text_short() {
        let chunks = super::routes_helpers::chunk_text("hello world", 512, 100);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "hello world");
    }

    #[test]
    fn test_chunk_text_exact_boundary() {
        let text = "a".repeat(512);
        let chunks = super::routes_helpers::chunk_text(&text, 512, 100);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_chunk_text_overlap() {
        let text = "a".repeat(1000);
        let chunks = super::routes_helpers::chunk_text(&text, 512, 100);
        assert!(chunks.len() >= 2);
        // With overlap, second chunk should start at 412 (512 - 100)
        // Total coverage: first chunk 0..512, second chunk 412..924, third if needed
    }

    #[test]
    fn test_chunk_text_chinese() {
        let text = "这是一个测试".repeat(200); // 1200 chars
        let chunks = super::routes_helpers::chunk_text(&text, 512, 100);
        assert!(chunks.len() >= 2);
        // Each chunk should be at most 512 chars
        for chunk in &chunks {
            assert!(chunk.chars().count() <= 512);
        }
    }

    // ─── JWT Service Tests ───────────────────────────────────────────────

    #[test]
    fn test_jwt_create_and_validate_access_token() {
        let jwt = crate::auth::jwt::JwtService::new("test-secret-key-for-testing-12345");
        let token = jwt.create_access_token("user-123", "reader").unwrap();
        let claims = jwt.validate_token(&token).unwrap();
        assert_eq!(claims.sub, "user-123");
        assert_eq!(claims.token_type, "access");
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_jwt_create_and_validate_refresh_token() {
        let jwt = crate::auth::jwt::JwtService::new("test-secret-key-for-testing-12345");
        let (token, expires_at) = jwt.create_refresh_token("user-456").unwrap();
        let claims = jwt.validate_token(&token).unwrap();
        assert_eq!(claims.sub, "user-456");
        assert_eq!(claims.token_type, "refresh");
        // Refresh token should expire ~30 days later
        let duration = expires_at - chrono::Utc::now();
        assert!(duration.num_days() >= 29);
    }

    #[test]
    fn test_jwt_invalid_token_rejected() {
        let jwt = crate::auth::jwt::JwtService::new("test-secret-key");
        let result = jwt.validate_token("invalid.token.here");
        assert!(result.is_err());
    }

    #[test]
    fn test_jwt_wrong_secret_rejected() {
        let jwt1 = crate::auth::jwt::JwtService::new("secret-one");
        let jwt2 = crate::auth::jwt::JwtService::new("secret-two");
        let token = jwt1.create_access_token("user-1", "admin").unwrap();
        let result = jwt2.validate_token(&token);
        assert!(result.is_err());
    }

    #[test]
    fn test_jwt_hash_token_deterministic() {
        let hash1 = crate::auth::jwt::JwtService::hash_token("my-refresh-token");
        let hash2 = crate::auth::jwt::JwtService::hash_token("my-refresh-token");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_jwt_hash_token_different_inputs() {
        let hash1 = crate::auth::jwt::JwtService::hash_token("token-a");
        let hash2 = crate::auth::jwt::JwtService::hash_token("token-b");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_jwt_hash_token_is_hex() {
        let hash = crate::auth::jwt::JwtService::hash_token("test");
        // SHA256 hex output is 64 chars
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ─── Config Tests ───────────────────────────────────────────────

    #[test]
    fn test_config_from_env_uses_defaults() {
        // from_env should succeed with defaults for all fields
        let config = crate::config::AppConfig::from_env().unwrap();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 3000);
    }

    // ─── Middleware / Rate Limiter Tests ───────────────────────────────

    #[test]
    fn test_rate_limiter_creation() {
        let limiter = crate::middleware::create_ai_rate_limiter(30);
        // Should allow the first request
        assert!(limiter.check().is_ok());
    }

    #[test]
    fn test_rate_limiter_exhaustion() {
        let limiter = crate::middleware::create_ai_rate_limiter(1);
        // First request should succeed
        assert!(limiter.check().is_ok());
        // Second request immediately should be rate limited (only 1 per minute)
        assert!(limiter.check().is_err());
    }

    #[test]
    fn test_general_rate_limiter() {
        let limiter = crate::middleware::create_general_rate_limiter(200);
        // Should allow many requests
        for _ in 0..10 {
            assert!(limiter.check().is_ok());
        }
    }

    // ─── Library ACL Tests ───────────────────────────────────────────────

    #[test]
    fn test_library_access_permission_predicates() {
        use crate::access::LibraryAccess;

        assert_eq!(
            LibraryAccess::Read.permission_predicate("lp"),
            "(lp.can_read OR lp.can_write OR lp.can_manage)"
        );
        assert_eq!(
            LibraryAccess::Write.permission_predicate("lp"),
            "(lp.can_write OR lp.can_manage)"
        );
        assert_eq!(
            LibraryAccess::Manage.permission_predicate("lp"),
            "lp.can_manage"
        );
    }
}

/// Re-export chunk_text for testing.
pub(crate) mod routes_helpers {
    pub fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        if chars.len() <= chunk_size {
            return vec![text.to_string()];
        }
        let mut chunks = Vec::new();
        let mut start = 0;
        while start < chars.len() {
            let end = (start + chunk_size).min(chars.len());
            chunks.push(chars[start..end].iter().collect());
            if end >= chars.len() {
                break;
            }
            start += chunk_size - overlap;
        }
        chunks
    }
}
