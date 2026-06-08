---
type: crate
language: rust
status: stable
path: crates/api
tags: [rust, llm, http, api]
---

# Rust-api

> **职责**:对接外部 LLM 厂商的 HTTP 客户端层。处理认证(OAuth/API Key)、请求构造、SSE 流式响应解析。本身不含任何统计逻辑。

## 在版图中的位置

依赖链顶端:[[Rust-api]] → [[Rust-agent-core]] → [[Rust-agent-server]]。
被 [[Rust-agent-core]] 的 [[LLM集成]] 使用。

## 模块结构 (`crates/api/src/`)

| 模块 | 作用 |
|------|------|
| `client.rs` | `ProviderClient` —— 统一的厂商客户端;`MessageStream` 流式消息 |
| `auth/` | OAuth PKCE 流程、凭证存取(`oauth.rs`) |
| `providers/` | 各厂商适配:`anthropic_provider`、`openai_compat`;模型别名解析、`max_tokens` |
| `sse.rs` | `SseParser` —— 解析 Server-Sent Events 流帧 |
| `types.rs` | 消息请求/响应、内容块、工具调用、流事件等核心数据类型 |
| `sidecar.rs` | sidecar 进程支持 |
| `error.rs` | `ApiError` |

## 关键类型

- `ProviderClient` / `AnthropicClient`(别名 `ApiClient`)/ `OpenAiCompatClient`
- `MessageRequest` / `MessageResponse` / `StreamEvent`
- `ToolDefinition` / `ToolChoice`(支持 LLM 工具调用)
- `OAuthTokenSet` / `PkceCodePair`(认证)

## 相关
- [[LLM集成]]
- [[TS-api]](TypeScript 对应层)
- [[ADR-0005-Power家族对齐SAS]](涉及厂商对接外的算法对齐)
