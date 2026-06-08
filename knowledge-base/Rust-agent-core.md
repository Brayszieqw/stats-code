---
type: crate
language: rust
status: stable
path: crates/agent-core
tags: [rust, llm, domain, orchestration]
---

# Rust-agent-core

> **职责**:业务编排核心。所有**纯领域逻辑** —— 会话状态机、校验、Skill 注册与参数校验、配额计算、错误码映射 —— 独立于任何 HTTP 框架。

## 在版图中的位置

中间层:被 [[Rust-agent-server]] 调用,向下使用 [[Rust-api]] 调 LLM、把统计请求路由给 [[Rust-stats-code]]。

> ⚠ TS 后端**没有**对应的独立 core 包(见 [[ADR-0004-TS三层包结构]]),职责落在 [[TS-server]] 内。

## 模块结构 (`crates/agent-core/src/`)

| 模块 | 作用 |
|------|------|
| `orchestrator.rs` | 编排器:驱动会话、消息流、工具调用循环 |
| `session_lifecycle.rs` | 会话生命周期状态机 |
| `skill/` | Skill 注册表 + 参数校验(把统计能力暴露给 LLM) |
| `llm/` | LLM 调用封装、模型选择 |
| `validation/` | 输入校验、`ChoicePrompt` 解析 |
| `store/` | 会话/消息存储 |
| `models/` | 领域数据模型 |
| `stt/` | 语音转文字(speech-to-text) |
| `sanitize.rs` / `encoding.rs` | 输入清洗、编码处理 |
| `traits/` | 抽象 trait 边界 |

## 设计要点

- **框架无关**:不依赖 axum,纯逻辑可独立测试(`tests/properties.rs` 用属性测试)。
- 通过 `SkillInvoker::StatsCli` 以子进程方式调用 [[Rust-stats-code]] 的算法子命令。

## 相关
- [[LLM集成]]
- [[统计引擎]]
- [[Rust-agent-server]]
- [[单命令启动器]]
