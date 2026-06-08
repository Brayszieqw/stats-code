---
type: adr
status: accepted
tags: [adr, typescript]
---

# ADR-0004:TS 后端三层包结构,移除空壳 core 包

> 移除 `packages/core`,TS 后端采用三层 `api → server → engine`。原 [[Rust-agent-core]] 的职责(SessionStore、LLM 配置、消息编排、SSE)全部落在 [[TS-server]] 内。

## 背景
最初按 Rust 4 crate 1:1 映射出 4 个包,但 `packages/core` 始终是空壳(只导出一个常量),无任何包 import 它。对它做 Deletion test → 删掉无影响。

## 决策
- `engine`:纯计算([[TS-engine]])
- `server`:HTTP 传输 + 编排([[TS-server]])
- `api`:应用组合 + SEA 入口([[TS-api]])

## 理由
1. **结构应反映现实**:空壳包是"假 seam"。
2. **YAGNI**:只有出现第二个非 HTTP 入口时,独立编排层才有价值。
3. 降低误导。

## 重新评估条件
- 新增第二个编排消费者(CLI/gRPC/批处理)→ 把编排抽进独立包。
- server 包膨胀到传输与编排互相干扰。

## 相关
- [[TS后端重写]] · [[TS-server]] · [[Rust-agent-core]]
