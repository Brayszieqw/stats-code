---
type: adr
status: accepted
tags: [adr, typescript]
---

# ADR-0004: TS 后端三层包结构，移除空壳 core 包

> 采用三层 `api → server → engine`。会话、LLM 配置、消息编排、SSE 均落在 [[TS-server]] 内，不设独立 core 包。

## 背景

曾出现空壳 `packages/core`（仅导出常量、无消费者）。Deletion test 证明可删。

## 决策

- `engine`：纯计算（[[TS-engine]]）
- `server`：HTTP 传输 + 编排（[[TS-server]]）
- `api`：应用组合 + SEA 入口（[[TS-api]]）

## 理由

1. 结构应反映现实：空壳包是假 seam。
2. YAGNI：只有出现第二个非 HTTP 入口时，独立编排层才有价值。

## 重新评估条件

- 新增第二个编排消费者（CLI / gRPC / 批处理）→ 再抽编排包。
- server 膨胀到传输与编排互相干扰。

## 相关

- [[TS后端重写]] · [[TS-server]] · [[TS-api]] · [[TS-engine]]
