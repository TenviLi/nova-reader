# Contributing to Nova Reader

## 开发环境搭建

### 前置条件

- **Rust**: 1.82+ (via rustup)
- **Node.js**: 22+ (推荐 fnm/volta)
- **pnpm**: 9+
- **Docker & Docker Compose**: 用于基础设施服务

### 快速搭建

```bash
# 1. 克隆
git clone https://github.com/yourname/nova-reader.git
cd nova-reader

# 2. 配置环境变量
cp .env.example .env

# 3. 启动依赖服务
docker compose up -d
# 等待所有服务健康 (约 10-30 秒)

# 4. 后端
cargo build   # 首次编译约 3-5 分钟
cargo run -p nova-api
# API 启动时自动执行数据库迁移

# 5. 前端
cd apps/web
pnpm install
pnpm dev
# → http://localhost:5173

# 6. (可选) 启动 Worker
cargo run -p nova-worker
```

### 首次使用

1. 访问 http://localhost:5173
2. 系统检测到无用户，自动跳转到初始设置
3. 创建管理员账户 (如: admin / Admin123!)
4. 配置书库路径

## 项目结构

```
nova-reader/
├── apps/web/            # SvelteKit 前端
│   ├── src/
│   │   ├── routes/      # 页面路由 (SvelteKit file-based routing)
│   │   ├── lib/
│   │   │   ├── components/  # UI 组件
│   │   │   ├── stores/      # Svelte 5 runes 状态管理
│   │   │   └── services/    # API 客户端
│   │   └── app.css      # TailwindCSS 4 全局样式
├── crates/
│   ├── nova-api/        # HTTP 服务 (Axum)
│   │   ├── src/routes/  # 路由模块 (每个文件 = 一组端点)
│   │   ├── src/repo/    # PostgreSQL Repository 实现
│   │   └── src/middleware/ # Auth, Rate limit, CORS
│   ├── nova-worker/     # 异步任务处理
│   ├── nova-ingest/     # 文件解析与摄入
│   ├── nova-core/       # 共享领域模型
│   └── ...
├── migrations/          # SQL 迁移 (sqlx, 自动执行)
├── docker-compose.yml   # 开发环境基础设施
└── scripts/             # 工具脚本 (seed.sql 等)
```

## 开发命令

```bash
# 后端
cargo build                          # 编译
cargo build --release -p nova-api    # Release 编译
cargo clippy                         # Lint
cargo test                           # 测试

# 前端
cd apps/web
pnpm dev          # 开发服务器 (HMR)
pnpm build        # 生产构建
pnpm check        # TypeScript 类型检查
pnpm test         # Vitest 单元测试

# 数据库
# 新增迁移: 创建文件 migrations/YYYYMMDDHHMMSS_description.sql
# 已应用的迁移文件不可重命名
# 迁移在 API 启动时自动执行 (state.rs → sqlx::migrate!)
```

## 代码规范

### Rust
- 使用 `clippy` 保持代码质量
- 错误处理使用 `ApiError` enum，通过 `?` 传播
- Repository pattern: 数据库访问通过 `PgXxxRepository` 封装
- 路由文件命名: `crates/nova-api/src/routes/{feature}.rs`
- 全局 durable task queue 使用 PostgreSQL `tasks` 表并原子 claim；Redis 当前仅用于缓存和 pub/sub，未来再统一迁移队列

### Frontend (Svelte 5)
- 使用 Runes (`$state`, `$derived`, `$effect`) 而非旧 stores
- 组件路径: `src/lib/components/{category}/{ComponentName}.svelte`
- API 调用通过 `$services/api` 统一封装
- TailwindCSS 4 工具类优先，使用 `@theme` 设计令牌

### 提交信息

```
feat: 添加书库批量导入功能
fix: 修复阅读进度保存失败
docs: 更新 API 文档
refactor: 重构 auth 中间件
```

## 添加新功能

### 添加新 API 端点

1. 创建或编辑 `crates/nova-api/src/routes/{feature}.rs`
2. 在 `routes/mod.rs` 中 `mod {feature}` + merge routes
3. 如需新表: 创建迁移文件
4. 如需新域类型: 在 `nova-core/src/domain/` 中定义

### 添加新前端页面

1. 创建 `apps/web/src/routes/{path}/+page.svelte`
2. 如需数据加载: 添加 `+page.ts` (TanStack Query)
3. 组件放在 `src/lib/components/`
