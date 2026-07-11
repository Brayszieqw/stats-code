---
type: package
language: typescript
status: stable
path: ts-backend/packages/api
tags: [typescript, api, sea]
---

# TS-api

> **职责**：应用组合层（依赖链顶端）。launcher + SEA 二进制入口。把 [[TS-server]] 与 [[TS-engine]] 组装为可运行的 `stats-code.exe`。

## 模块（`packages/api/src/`）

| 文件 | 作用 |
|------|------|
| `bin.ts` | SEA 二进制入口 |
| `launcher.ts` | 启动器（端口、浏览器、单实例锁） |
| `index.ts` | 包导出 |

## 说明

LLM 客户端不在本包，而在 [[TS-server]]。本包只负责进程生命周期与产物入口。

## 产物

`npm run sea` → Node SEA → 单文件 `stats-code.exe`（内嵌运行时 + 前端资源）。

## 相关

- [[TS-server]] · [[TS-engine]] · [[单命令启动器]] · [[ADR-0001-单文件exe分发]]
