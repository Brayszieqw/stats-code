---
type: crate
language: rust
status: stable
path: crates/agent-server
tags: [rust, http, axum, server]
---

# Rust-agent-server

> **职责**:axum HTTP 服务。对前端暴露 REST 路由,接请求、调 [[Rust-agent-core]] 编排、返回结果。生产模式下还内嵌前端静态资源(SPA fallback)。

## 在版图中的位置

最外层(对前端):[[Web-前端]] → HTTP → [[Rust-agent-server]] → [[Rust-agent-core]]。

## 路由表 (`src/lib.rs::build_router`)

| 方法 | 路径 | 处理器 |
|------|------|--------|
| GET | `/api/health` | 健康检查 |
| POST | `/api/sessions` | 创建会话 |
| GET | `/api/sessions/:sid` | 取会话 |
| PATCH | `/api/sessions/:sid/settings` | 改设置 |
| POST | `/api/sessions/:sid/messages` | 发消息(核心) |
| POST | `/api/sessions/:sid/audio` | 上传音频(限 10 MiB) |
| POST | `/api/sessions/:sid/datasets` | 上传数据集(限 70 MiB) |
| GET | `/api/sessions/:sid/datasets/:did` | 取数据集 |
| GET/POST | `/api/llm-status` · `/api/llm-config` | LLM 配置 |
| GET | `/api/coverage-matrix` | 算法覆盖矩阵([[Parity与Sidecar]]) |
| POST | `/api/sidecar/:algorithm_id` | 等价代码侧栏 |
| POST | `/api/snapshot/export` | [[审计快照]]导出 |

## 模块结构 (`src/`)

| 模块 | 作用 |
|------|------|
| `handlers/` | 各路由的处理器 |
| `middleware/` | CORS、负载卸载(load shedding)、请求 ID |
| `state.rs` | `AppState` 共享状态 |
| `orchestrator_adapter.rs` | 把 HTTP 请求适配到 [[Rust-agent-core]] 编排器 |
| `config.rs` / `error.rs` | 配置加载、错误类型 |

## 中间件顺序

CORS(最外)→ 负载卸载(写 `X-Server-Load` 头)→ 请求 ID(生成 UUID + tracing span)。

## 模式切换

- **prod**:`fallback` 从内嵌 `web/dist/` 提供前端(`rust-embed`)。
- **dev-vite** feature:跳过 fallback,请求透传给 Vite dev server(见 [[单命令启动器]])。

## 相关
- [[TS-server]](TypeScript 对应层)
- [[Web-前端]]
