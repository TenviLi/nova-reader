use serde::{Deserialize, Serialize};

use nova_core::domain::search::EntityType;

/// Entity extraction schema for LLM-based knowledge graph construction.
/// This defines the strict JSON schema that DeepSeek must output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub entities: Vec<ExtractedEntity>,
    pub relationships: Vec<ExtractedRelationship>,
}

/// An entity extracted from text by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedEntity {
    pub name: String,
    /// Canonical name (for deduplication/disambiguation)
    pub canonical_name: Option<String>,
    pub entity_type: EntityType,
    pub description: String,
    /// Alternative names/aliases for this entity
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Key attributes
    #[serde(default)]
    pub attributes: serde_json::Value,
}

/// A relationship between two entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedRelationship {
    pub source: String,
    pub target: String,
    pub relation_type: String,
    pub description: String,
    /// Strength/confidence of this relationship (0.0-1.0)
    #[serde(default = "default_weight")]
    pub weight: f64,
    /// Whether this relationship is directional
    #[serde(default = "default_true")]
    pub directed: bool,
}

fn default_weight() -> f64 {
    1.0
}
fn default_true() -> bool {
    true
}

/// The prompt template for entity extraction.
pub const ENTITY_EXTRACTION_PROMPT: &str = r#"你是一个专业的文学分析师。请从以下小说文本中提取所有有意义的实体和关系。

要求：
1. 实体类型包括：character（人物）、location（地点）、organization（组织/门派）、event（事件）、item（物品/法宝）、concept（概念/功法）、technique（技能/招式）、timeline（时间点）
2. 关系类型包括但不限于：师徒、敌对、同门、从属、拥有、发生于、位于、使用、创造、击败
3. 注意人物的别名/外号/代称，统一到 canonical_name
4. 每个实体的 description 应该简洁但信息丰富

请严格按照以下 JSON 格式输出：
```json
{
  "entities": [
    {
      "name": "实体名",
      "canonical_name": "标准名（如有别名）",
      "entity_type": "character|location|...",
      "description": "简短描述",
      "aliases": ["别名1", "别名2"],
      "attributes": {"key": "value"}
    }
  ],
  "relationships": [
    {
      "source": "源实体名",
      "target": "目标实体名",
      "relation_type": "关系类型",
      "description": "关系描述",
      "weight": 1.0,
      "directed": true
    }
  ]
}
```

文本内容：
{text}
"#;

/// Japanese-specific entity extraction prompt.
/// Handles: honorifics (さん/くん/様/先生), family→given name order,
/// katakana names, light novel terminology.
pub const ENTITY_EXTRACTION_PROMPT_JA: &str = r#"あなたはプロの文学アナリストです。以下のライトノベル/小説テキストから、意味のあるエンティティと関係性を抽出してください。

要件：
1. エンティティタイプ：character（人物）、location（場所）、organization（組織/ギルド）、event（イベント）、item（アイテム/武器）、concept（概念/魔法体系）、technique（スキル/技）、timeline（時間）
2. 関係タイプ：師弟、敵対、同僚、所属、所有、発生場所、使用、創造、討伐、恋愛
3. 人物名の注意点：
   - 敬称（さん/くん/様/先輩/先生）を除いた本名を canonical_name に設定
   - 姓と名が別々に言及される場合はフルネームに統合
   - カタカナ名（外国人キャラ等）はそのまま保持
   - あだ名/二つ名/称号は aliases に追加
4. 各エンティティの description は簡潔だが情報量のある日本語で

以下のJSON形式で厳密に出力してください：
```json
{
  "entities": [
    {
      "name": "エンティティ名",
      "canonical_name": "標準名（敬称なし・フルネーム）",
      "entity_type": "character|location|...",
      "description": "簡潔な説明",
      "aliases": ["別名1", "あだ名"],
      "attributes": {"key": "value"}
    }
  ],
  "relationships": [
    {
      "source": "ソースエンティティ",
      "target": "ターゲットエンティティ",
      "relation_type": "関係タイプ",
      "description": "関係の説明",
      "weight": 1.0,
      "directed": true
    }
  ]
}
```

テキスト内容：
{text}
"#;

/// Select the appropriate extraction prompt based on detected language.
pub fn get_extraction_prompt(language: &str) -> &'static str {
    match language {
        "japanese" | "ja" => ENTITY_EXTRACTION_PROMPT_JA,
        _ => ENTITY_EXTRACTION_PROMPT,
    }
}
