#[cfg(test)]
mod agentic_tests {
    use crate::agentic::{AgenticGraphRag, GraphRagResult, RetrievalStrategy};

    #[test]
    fn local_strategy_generates_single_cypher_query() {
        let rag = AgenticGraphRag::new(RetrievalStrategy::Local {
            entity_names: vec!["张三".to_string(), "李四".to_string()],
        });
        let queries = rag.get_cypher_queries("book-123");
        assert_eq!(queries.len(), 1);
        assert!(queries[0].0.contains("WHERE n.name IN $names"));
        assert_eq!(queries[0].1["book_id"], "book-123");
        assert_eq!(queries[0].1["names"][0], "张三");
        assert_eq!(queries[0].1["names"][1], "李四");
    }

    #[test]
    fn global_strategy_queries_community_summaries() {
        let rag = AgenticGraphRag::new(RetrievalStrategy::Global {
            community_level: 2,
        });
        let queries = rag.get_cypher_queries("book-456");
        assert_eq!(queries.len(), 1);
        assert!(queries[0].0.contains("Community"));
        assert_eq!(queries[0].1["level"], 2);
    }

    #[test]
    fn multi_hop_strategy_uses_shortest_path() {
        let rag = AgenticGraphRag::new(RetrievalStrategy::MultiHop {
            source: "主角".to_string(),
            target: "反派".to_string(),
            max_hops: 3,
        });
        let queries = rag.get_cypher_queries("book-789");
        assert_eq!(queries.len(), 1);
        assert!(queries[0].0.contains("shortestPath"));
        assert!(queries[0].0.contains("*..3"));
        assert_eq!(queries[0].1["source"], "主角");
        assert_eq!(queries[0].1["target"], "反派");
    }

    #[test]
    fn hybrid_strategy_without_community_generates_one_query() {
        let rag = AgenticGraphRag::new(RetrievalStrategy::Hybrid {
            entity_names: vec!["实体A".to_string()],
            include_community: false,
        });
        let queries = rag.get_cypher_queries("book-1");
        assert_eq!(queries.len(), 1);
        assert!(queries[0].0.contains("n.name IN $names"));
    }

    #[test]
    fn hybrid_strategy_with_community_generates_two_queries() {
        let rag = AgenticGraphRag::new(RetrievalStrategy::Hybrid {
            entity_names: vec!["实体A".to_string(), "实体B".to_string()],
            include_community: true,
        });
        let queries = rag.get_cypher_queries("book-2");
        assert_eq!(queries.len(), 2);
        assert!(queries[1].0.contains("Community"));
    }

    #[test]
    fn retrieval_strategy_serde_roundtrip() {
        let strategies = vec![
            RetrievalStrategy::Local {
                entity_names: vec!["x".into()],
            },
            RetrievalStrategy::Global { community_level: 1 },
            RetrievalStrategy::MultiHop {
                source: "a".into(),
                target: "b".into(),
                max_hops: 5,
            },
            RetrievalStrategy::Hybrid {
                entity_names: vec!["y".into()],
                include_community: true,
            },
        ];

        for strategy in strategies {
            let json = serde_json::to_string(&strategy).unwrap();
            let deserialized: RetrievalStrategy = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&deserialized).unwrap();
            assert_eq!(json, json2);
        }
    }

    #[test]
    fn graph_rag_result_serde() {
        let result = GraphRagResult {
            strategy_used: "local".to_string(),
            context: "张三是主角".to_string(),
            entities_involved: vec!["张三".to_string()],
            community_summaries: vec![],
            paths: vec![vec!["张三".into(), "李四".into()]],
            confidence: 0.85,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: GraphRagResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.strategy_used, "local");
        assert_eq!(deserialized.confidence, 0.85);
        assert_eq!(deserialized.paths.len(), 1);
    }

    #[test]
    fn planning_prompt_contains_required_placeholders() {
        assert!(crate::agentic::PLANNING_PROMPT.contains("{question}"));
        assert!(crate::agentic::PLANNING_PROMPT.contains("{known_entities}"));
    }

    #[test]
    fn aggregation_prompt_contains_context_placeholders() {
        assert!(crate::agentic::AGGREGATION_PROMPT.contains("{entity_context}"));
        assert!(crate::agentic::AGGREGATION_PROMPT.contains("{community_context}"));
        assert!(crate::agentic::AGGREGATION_PROMPT.contains("{path_context}"));
    }
}

#[cfg(test)]
mod entity_tests {
    use crate::entity::{ExtractionResult, ExtractedEntity, ExtractedRelationship};
    use nova_core::domain::search::EntityType;

    #[test]
    fn extraction_result_serde_roundtrip() {
        let result = ExtractionResult {
            entities: vec![ExtractedEntity {
                name: "林动".to_string(),
                canonical_name: Some("林动".to_string()),
                entity_type: EntityType::Character,
                description: "主角，少年天才".to_string(),
                aliases: vec!["小动".to_string()],
                attributes: serde_json::json!({"age": 16}),
            }],
            relationships: vec![ExtractedRelationship {
                source: "林动".to_string(),
                target: "林氏家族".to_string(),
                relation_type: "属于".to_string(),
                description: "林氏家族成员".to_string(),
                weight: 0.9,
                directed: true,
            }],
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: ExtractionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.entities.len(), 1);
        assert_eq!(deserialized.entities[0].name, "林动");
        assert_eq!(deserialized.entities[0].aliases, vec!["小动"]);
        assert_eq!(deserialized.relationships.len(), 1);
        assert_eq!(deserialized.relationships[0].weight, 0.9);
    }

    #[test]
    fn extracted_relationship_defaults() {
        let json = r#"{
            "source": "A",
            "target": "B",
            "relation_type": "friend",
            "description": "friends"
        }"#;
        let rel: ExtractedRelationship = serde_json::from_str(json).unwrap();
        assert_eq!(rel.weight, 1.0); // default_weight
        assert!(rel.directed); // default_true
    }

    #[test]
    fn entity_extraction_prompt_contains_json_schema() {
        assert!(crate::entity::ENTITY_EXTRACTION_PROMPT.contains("entities"));
        assert!(crate::entity::ENTITY_EXTRACTION_PROMPT.contains("relationships"));
        assert!(crate::entity::ENTITY_EXTRACTION_PROMPT.contains("{text}"));
    }
}

#[cfg(test)]
mod community_tests {
    use crate::community::Community;

    #[test]
    fn community_serde() {
        let community = Community {
            id: "c-1".to_string(),
            level: 1,
            entities: vec!["张三".into(), "李四".into()],
            summary: "两人是搭档".to_string(),
            key_findings: vec!["合作紧密".into(), "互相信任".into()],
            parent_community: None,
        };

        let json = serde_json::to_string(&community).unwrap();
        let deserialized: Community = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.entities.len(), 2);
        assert_eq!(deserialized.level, 1);
        assert!(deserialized.parent_community.is_none());
    }

    #[test]
    fn community_with_parent() {
        let community = Community {
            id: "c-2".to_string(),
            level: 2,
            entities: vec!["A".into()],
            summary: "子社区".to_string(),
            key_findings: vec![],
            parent_community: Some("c-1".to_string()),
        };

        assert_eq!(community.parent_community.as_deref(), Some("c-1"));
    }

    #[test]
    fn community_summary_prompt_has_placeholders() {
        assert!(crate::community::COMMUNITY_SUMMARY_PROMPT.contains("{entities}"));
        assert!(crate::community::COMMUNITY_SUMMARY_PROMPT.contains("{relationships}"));
    }
}
