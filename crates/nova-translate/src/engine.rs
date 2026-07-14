use crate::glossary::GlossaryManager;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Translation engine using DeepSeek with glossary-enforced prompts.
pub struct TranslationEngine {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
    glossary: GlossaryManager,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationRequest {
    pub text: String,
    pub source_language: String,
    pub target_language: String,
    pub book_id: Option<uuid::Uuid>,
    /// Additional style/tone instructions
    pub style_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResult {
    pub translated_text: String,
    pub detected_terms: Vec<String>,
    pub confidence: f64,
}

impl TranslationEngine {
    pub fn new(api_key: String, base_url: String, model: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url,
            model,
            glossary: GlossaryManager::new(),
        }
    }

    /// Translate text with glossary-aware context injection.
    pub async fn translate(&self, request: &TranslationRequest) -> nova_core::Result<TranslationResult> {
        // 1. Retrieve relevant glossary entries
        let entries = if let Some(book_id) = &request.book_id {
            self.glossary.get_entries_for_book(book_id).await?
        } else {
            Vec::new()
        };

        // 2. Build the translation prompt with glossary context
        let detected_terms = entries
            .iter()
            .filter(|entry| request.text.contains(&entry.source_term))
            .map(|entry| entry.source_term.clone())
            .collect();
        let glossary_context = GlossaryManager::format_as_context(&entries);
        let prompt = self.build_prompt(request, &glossary_context);

        // 3. Call DeepSeek API
        let response = self.call_llm(&prompt).await?;

        Ok(TranslationResult {
            translated_text: response,
            detected_terms,
            confidence: 0.95,
        })
    }

    fn build_prompt(&self, request: &TranslationRequest, glossary_context: &str) -> String {
        let style_instruction = request
            .style_notes
            .as_deref()
            .unwrap_or("保持原文的文学风格和语气");

        format!(
            r#"你是一位专业的文学翻译家，精通{source}和{target}。

{glossary}

## 翻译要求：
1. 将以下{source}文本翻译为{target}
2. 严格使用术语表中的对应译名
3. 保持原文的段落结构和标点习惯
4. {style}
5. 不要添加任何解释或注释，只输出翻译结果

## 原文：
{text}

## 译文：
"#,
            source = request.source_language,
            target = request.target_language,
            glossary = glossary_context,
            style = style_instruction,
            text = request.text,
        )
    }

    /// Test-accessible version of build_prompt.
    #[cfg(test)]
    pub fn build_prompt_public(&self, request: &TranslationRequest, glossary_context: &str) -> String {
        self.build_prompt(request, glossary_context)
    }

    async fn call_llm(&self, prompt: &str) -> nova_core::Result<String> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "user", "content": prompt}
            ],
            "temperature": 0.3,
            "max_tokens": 4096,
        });

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| nova_core::Error::AiService(e.to_string()))?;

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| nova_core::Error::AiService(e.to_string()))?;

        json["choices"][0]["message"]["content"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| nova_core::Error::AiService("Invalid LLM response format".to_string()))
    }
}
