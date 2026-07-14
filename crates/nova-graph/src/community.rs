/// Community detection and summarization for GraphRAG.
///
/// Uses Leiden algorithm concepts for hierarchical community clustering
/// on the entity relationship graph. Each community gets a summary
/// that enables fast global-scope question answering.

use serde::{Deserialize, Serialize};

/// A community of tightly-connected entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Community {
    pub id: String,
    pub level: i32,
    pub entities: Vec<String>,
    pub summary: String,
    pub key_findings: Vec<String>,
    pub parent_community: Option<String>,
}

/// Prompt for generating community summaries.
pub const COMMUNITY_SUMMARY_PROMPT: &str = r#"你是一个文学分析师。以下是一组在小说中紧密关联的实体及其关系。
请生成一个简洁的社区摘要，概括这组实体的核心故事线和主要互动。

实体列表：
{entities}

关系列表：
{relationships}

请输出：
1. 一段 2-3 句话的总结
2. 3-5 个关键发现

格式：
```json
{
  "summary": "社区总结...",
  "key_findings": ["发现1", "发现2", "发现3"]
}
```
"#;
