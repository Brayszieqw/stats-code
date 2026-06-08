---
type: package
language: typescript
status: in-progress
path: ts-backend/packages/server
tags: [typescript, http, fastify, orchestration]
---

# TS-server

> **职责**:HTTP 传输 **+ 编排**。Fastify 路由、契约校验、SSE、SPA 兜底,以及原本属于 [[Rust-agent-core]] 的会话/消息编排/LLM 配置职责(因为 TS 版没有独立 core 包)。

## 模块 (`packages/server/src/`)

| 文件 | 作用 | Rust 对应 |
|------|------|-----------|
| `router.ts` | Fastify 路由定义 | [[Rust-agent-server]] `handlers/` |
| `state.ts` | `AppState` 共享状态 | agent-server `state.rs` |
| `mem-store.ts` | 会话/消息内存存储 | [[Rust-agent-core]] `store/` |
| `llm.ts` | LLM 调用封装 | agent-core `llm/` |
| `providers.ts` | LLM 厂商接线 | [[Rust-api]] `providers/` |
| `sse.ts` | SSE 流式帧 | api `sse.rs` |
| `spa.ts` · `spa-assets.ts` | SPA 兜底 + 内嵌前端资源 | agent-server static_assets |
| `contract/` | 请求/响应契约校验 | — |

## 关键点

- **承载两类关注点**(传输 + 编排):这是 [[ADR-0004-TS三层包结构]] 的有意决策 —— 因为目前只有 HTTP 一个入口,编排与传输同包合理。
- 直接 import [[TS-engine]] 的纯函数算法,**不 spawn 子进程**(见 [[ADR-0003-TS后端进程内算法]])。

## 相关
- [[TS后端重写]] · [[TS-api]] · [[TS-engine]]
- [[LLM集成]]
