#[cfg(test)]
mod glossary_tests {
    use crate::glossary::{GlossaryEntry, GlossaryManager, TermCategory};
    use chrono::Utc;
    use uuid::Uuid;

    fn make_entry(source: &str, target: &str, category: TermCategory) -> GlossaryEntry {
        GlossaryEntry {
            id: Uuid::now_v7(),
            book_id: None,
            source_term: source.to_string(),
            target_term: target.to_string(),
            category,
            context: None,
            is_global: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn format_as_context_empty_returns_empty() {
        let result = GlossaryManager::format_as_context(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn format_as_context_single_entry() {
        let entries = vec![make_entry("Cultivation", "修炼", TermCategory::Concept)];
        let result = GlossaryManager::format_as_context(&entries);
        assert!(result.contains("Cultivation"));
        assert!(result.contains("修炼"));
        assert!(result.contains("Concept"));
        assert!(result.contains("术语表"));
    }

    #[test]
    fn format_as_context_multiple_entries() {
        let entries = vec![
            make_entry("Lin Dong", "林动", TermCategory::CharacterName),
            make_entry("Qingyang Town", "青阳镇", TermCategory::Location),
            make_entry("Nine Heavens Sect", "九天宗", TermCategory::Organization),
        ];
        let result = GlossaryManager::format_as_context(&entries);
        assert!(result.contains("Lin Dong"));
        assert!(result.contains("林动"));
        assert!(result.contains("青阳镇"));
        assert!(result.contains("九天宗"));
        // Should have table headers
        assert!(result.contains("| 原文 | 译文 | 类别 |"));
    }

    #[test]
    fn format_as_context_includes_usage_instruction() {
        let entries = vec![make_entry("x", "y", TermCategory::Item)];
        let result = GlossaryManager::format_as_context(&entries);
        assert!(result.contains("严格使用"));
    }

    #[test]
    fn term_category_serde_roundtrip() {
        let categories = vec![
            TermCategory::CharacterName,
            TermCategory::Location,
            TermCategory::Organization,
            TermCategory::Technique,
            TermCategory::Item,
            TermCategory::Title,
            TermCategory::Concept,
            TermCategory::Other,
        ];
        for cat in categories {
            let json = serde_json::to_string(&cat).unwrap();
            let deserialized: TermCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(cat, deserialized);
        }
    }

    #[test]
    fn term_category_snake_case_serialization() {
        let json = serde_json::to_string(&TermCategory::CharacterName).unwrap();
        assert_eq!(json, r#""character_name""#);

        let json = serde_json::to_string(&TermCategory::Organization).unwrap();
        assert_eq!(json, r#""organization""#);
    }

    #[test]
    fn glossary_entry_serde_roundtrip() {
        let entry = GlossaryEntry {
            id: Uuid::now_v7(),
            book_id: Some(Uuid::now_v7()),
            source_term: "Heavenly Dao".to_string(),
            target_term: "天道".to_string(),
            category: TermCategory::Concept,
            context: Some("Used in cultivation context".to_string()),
            is_global: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: GlossaryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.source_term, "Heavenly Dao");
        assert_eq!(deserialized.target_term, "天道");
        assert_eq!(deserialized.category, TermCategory::Concept);
        assert!(!deserialized.is_global);
        assert!(deserialized.book_id.is_some());
    }

    #[test]
    fn glossary_manager_can_be_constructed() {
        let _manager = GlossaryManager::new();
        // Smoke test - construction should not panic
    }
}

#[cfg(test)]
mod engine_tests {
    use crate::engine::{TranslationEngine, TranslationRequest, TranslationResult};

    #[test]
    fn translation_request_serde() {
        let req = TranslationRequest {
            text: "Hello world".to_string(),
            source_language: "en".to_string(),
            target_language: "zh-CN".to_string(),
            book_id: None,
            style_notes: Some("文学翻译".to_string()),
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: TranslationRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.text, "Hello world");
        assert_eq!(deserialized.target_language, "zh-CN");
        assert_eq!(deserialized.style_notes.as_deref(), Some("文学翻译"));
    }

    #[test]
    fn translation_result_serde() {
        let result = TranslationResult {
            translated_text: "你好世界".to_string(),
            detected_terms: vec!["Hello".to_string()],
            confidence: 0.95,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: TranslationResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.translated_text, "你好世界");
        assert_eq!(deserialized.confidence, 0.95);
    }

    #[test]
    fn translation_engine_can_be_constructed() {
        let _engine = TranslationEngine::new(
            "sk-test".to_string(),
            "https://api.deepseek.com".to_string(),
            "deepseek-chat".to_string(),
        );
    }

    #[test]
    fn build_prompt_includes_glossary() {
        let engine = TranslationEngine::new(
            "sk-test".to_string(),
            "https://api.deepseek.com".to_string(),
            "deepseek-chat".to_string(),
        );

        let request = TranslationRequest {
            text: "test text".to_string(),
            source_language: "英文".to_string(),
            target_language: "中文".to_string(),
            book_id: None,
            style_notes: None,
        };

        let prompt = engine.build_prompt_public(&request, "| Hello | 你好 | Concept |");
        assert!(prompt.contains("英文"));
        assert!(prompt.contains("中文"));
        assert!(prompt.contains("test text"));
        assert!(prompt.contains("Hello"));
    }
}
