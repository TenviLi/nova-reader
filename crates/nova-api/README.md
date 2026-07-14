# nova-api

Nova Reader 的 HTTP API 服务器，基于 Axum 0.8 构建。

## 职责

- RESTful API 路由 (18+ 模块)
- JWT 认证 + 刷新令牌
- SSE 流式响应 (AI chat, 任务进度)
- 文件上传 (multipart)
- CORS, 压缩, 限流中间件

## 路由模块

| 模块 | 路径 | 功能 |
|------|------|------|
| `books` | `/api/books` | 图书 CRUD, 去重检测, 封面提取 |
| `chapters` | `/api/chapters` | 章节内容, 分割, AI 标注 |
| `libraries` | `/api/libraries` | 书库管理, 扫描触发, 分析 |
| `series` | `/api/series` | 系列管理, 卷排序 |
| `persons` | `/api/persons` | 人物/作者管理 |
| `entities` | `/api/entities` | 实体 CRUD, 关系图 |
| `search` | `/api/search` | 混合搜索 (关键词 + 语义 + 图) |
| `ai` | `/api/ai` | Chat/摘要/实体抽取/风格分析/大纲生成 |
| `translate` | `/api/translate` | 术语表翻译, 批量翻译 |
| `auth` | `/api/auth` | 登录/注册/刷新/登出 |
| `users` | `/api/users` | 用户管理 |
| `stats` | `/api/stats` | 阅读统计, 热力图 |
| `tasks` | `/api/tasks` | 后台任务队列, 进度查询 |
| `recommendations` | `/api/recommendations` | AI 推荐 |
| `import_export` | 未挂载 | Calibre/OPDS 导入导出（规划中） |
| `opds` | 未挂载 | OPDS feed 外部阅读器（规划中） |

## AI 服务 (`ai_service.rs`)

使用 `async-openai` SDK 与 DeepSeek/OpenAI 兼容 API 通信：
- 类型安全的请求/响应模型
- 自动重试 + 指数退避
- **全书 AI 流水线**: 一次性处理整本小说 (摘要+实体+标签+风格分析)

## 运行

```bash
cargo run --bin nova-api
# 或指定配置
NOVA_DATABASE_URL=postgres://... cargo run --bin nova-api
```
