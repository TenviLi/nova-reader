# Changelog

All notable changes to this project will be documented in this file.

Format based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added
- **AI 功能实装**: 22+ AI 端点 (chat, summarize, entities, sentiment, style-transfer, etc.)
- **AI 流式响应**: SSE streaming for chat endpoint
- **Redis 缓存**: Dashboard stats 60s cache
- **数据库 reconciliation**: Migration 019 修复所有 struct/column 不匹配
- **文档**: 架构设计、部署指南、API 文档、贡献指南
- **Issue/PR templates**: GitHub 模板

### Fixed
- Login 页面不再显示 sidebar/topnav
- Setup 链接在初始化完成后自动隐藏
- Entity mentions `book_id`/`position_*` 字段类型修复
- AI routes 编译错误 (9 errors → 0)

### Infrastructure
- 自动数据库迁移 (sqlx::migrate! at startup)
- 结构化 JSON 日志 (production mode)
- 健康检查 (/health/ready checks all dependencies)

## [0.1.0] - 2025-01-XX

### Added
- 核心书库管理 (上传、浏览、搜索)
- 阅读器 (章节渲染、进度追踪、书签、批注)
- 用户认证 (JWT + HTTP-only cookies)
- 知识图谱可视化 (实体关系 Canvas 图)
- 统计仪表盘 (阅读热力图、连续天数、目标)
- 多视图书库 (Grid/List/Table/Timeline)
- 暗色/亮色主题
- 命令面板 (⌘K)
- 移动端响应式
- Docker 部署支持
