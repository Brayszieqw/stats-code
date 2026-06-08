---
type: package
language: typescript
status: in-progress
path: ts-backend/packages/api
tags: [typescript, api, sea]
---

# TS-api

> **职责**:应用组合层(依赖链顶端)。launcher runner + SEA 二进制入口。把 [[TS-server]] 和 [[TS-engine]] 组装成可运行的 `stats-code.exe`。

## 模块 (`packages/api/src/`)

| 文件 | 作用 |
|------|------|
| `bin.ts` | SEA 二进制入口点 |
| `launcher.ts` | launcher 运行器(端口、浏览器、进程,见 [[单命令启动器]]) |
| `index.ts` | 包导出 |

## 对应关系
- 对应 Rust 的 [[Rust-api]],但职责不同:Rust-api 是 LLM 客户端层;TS-api 是应用组合/入口层(LLM 客户端在 TS 版落到 [[TS-server]] 的 `llm.ts` / `providers.ts`)。

## 产物
`npm run sea` → Node SEA blob 注入 → 单文件 `stats-code.exe`,内嵌运行时 + 前端资源,零外部依赖。

## 相关
- [[TS后端重写]] · [[TS-server]] · [[TS-engine]]
- [[ADR-0001-单文件exe分发]]
