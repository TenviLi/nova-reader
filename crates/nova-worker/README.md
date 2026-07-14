# nova-worker

Redis-backed 后台任务队列 (inspired by Asynq/Sidekiq)。

## 职责

- **任务定义**: 类型安全的任务 payload
- **优先级队列**: 高/中/低 三级优先
- **自动重试**: 指数退避, 最大重试次数
- **死信队列**: 失败任务归档供排查
- **并发控制**: 可配置 worker 数量

## 任务类型

| 任务 | 优先级 | 说明 |
|------|--------|------|
| `ScanLibrary` | 高 | 扫描书库文件系统 |
| `IngestBook` | 中 | 解析/分章/清理图书内容 |
| `GenerateEmbeddings` | 低 | 生成文本向量存入 Qdrant |
| `ExtractEntities` | 中 | AI 实体抽取 → Neo4j |
| `TranslateChapter` | 低 | 逐章翻译 |
| `AnalyzeStyle` | 低 | 写作风格分析 |
| `BuildIndex` | 中 | 建立 Meilisearch 索引 |

## 使用

```rust
use nova_worker::{TaskQueue, Task, TaskPriority};

let queue = TaskQueue::new(redis_client);
queue.enqueue(Task::ScanLibrary { library_id }, TaskPriority::High).await?;
```
