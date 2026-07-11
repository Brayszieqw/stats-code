---
type: package
language: typescript
status: stable
path: ts-backend/packages/server
tags: [typescript, http, fastify, orchestration]
---

# TS-server

> **职责**：HTTP 传输 **+ 编排**。Fastify 路由、契约校验、SSE、SPA 兜底，以及会话 / 消息编排 / LLM 配置 / skill 执行。

## 模块（`packages/server/src/`）

| 区域 | 作用 |
|------|------|
| `router.ts` | Fastify 路由 |
| `state.ts` | `AppState` 共享状态 |
| 会话 / store | 会话与消息持久化 |
| LLM / providers | 厂商接线与调用 |
| `sse.ts` | SSE 流式帧 |
| skill registry/runner | 解析 skill → 调 [[TS-engine]] |
| contract/ | 请求/响应 zod 契约 |
| spa / spa-assets | SPA 兜底与内嵌资源 |

## 关键点

- 传输与编排同包：见 [[ADR-0004-TS三层包结构]]（当前仅 HTTP 一个入口）。
- 直接 import [[TS-engine]] 纯函数，**不 spawn 统计子进程**（[[ADR-0003-TS后端进程内算法]]）。

## 相关

- [[TS-api]] · [[TS-engine]] · [[LLM集成]] · [[Web-前端]]
