# nova-translate

术语表驱动的文学翻译引擎，支持沉浸式双语阅读。

## 职责

- **术语表管理** (`GlossaryManager`): 维护专有名词对照表, 确保跨章节一致性
- **翻译引擎** (`TranslationEngine`): DeepSeek 驱动的翻译, 自动注入术语表上下文
- **批量翻译**: 整本书逐章翻译, 保持术语统一

## 翻译质量保证

1. **术语强制**: 翻译前检索相关术语表条目注入 prompt
2. **上下文窗口**: 携带前后章节摘要避免割裂
3. **风格一致**: 通过 `style_notes` 参数控制译文风格
4. **后处理校验**: 验证术语表中的名词是否正确使用

## 沉浸式翻译模式

灵感来自「沉浸式翻译」(ImmerseTranslate):
- **双语对照**: 原文段落 + 译文段落交替显示
- **仅译文**: 隐藏原文, 只看翻译
- **原文优先**: 译文以淡色 tooltip 辅助
- **逐段翻译**: 按需翻译可见段落 (懒加载)
- **术语高亮**: 术语表中的词汇特殊标记

## 使用

```rust
use nova_translate::{TranslationEngine, TranslationRequest};

let engine = TranslationEngine::new(api_key, base_url, model);
let result = engine.translate(&TranslationRequest {
    text: "修仙界中...",
    source_language: "zh".into(),
    target_language: "en".into(),
    book_id: Some(uuid),
    style_notes: Some("保持仙侠文风".into()),
}).await?;
```
