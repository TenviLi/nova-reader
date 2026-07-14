# nova-embed

嵌入向量生成与向量运算。

## 职责

- **文本嵌入**: 调用本地 Qwen3-Embedding (MLX) 生成稠密向量
- **文本分块**: 512 token 分块, 50-100 token 重叠
- **向量运算**: 余弦相似度、MinHash LSH、SemDeDup

## 嵌入模型

- **模型**: Qwen3-Embedding (qwen/Qwen3-Embedding-0.6B via MLX)
- **维度**: 1024
- **部署**: 本地 HTTP 服务 (`EMBEDDING_ENDPOINT`, 默认 `http://localhost:8999`)
- **硬件**: Apple Silicon (M1/M2/M3/M4) 通过 MLX 加速

## 使用

```rust
use nova_embed::EmbeddingClient;

let client = EmbeddingClient::new("http://localhost:8999");
let vectors = client.embed_texts(&["第一章内容...", "第二章内容..."]).await?;
```
