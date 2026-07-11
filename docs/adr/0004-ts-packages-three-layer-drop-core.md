# ADR-0004：TS 后端三层包结构（api → server → engine）

**状态**：accepted

## 决策

不设空壳 `packages/core`。三层职责：

| 包 | 职责 |
|----|------|
| `engine` | 纯计算、sidecar、snapshot、spawn_policy |
| `server` | HTTP + 会话/LLM/skill 编排 |
| `api` | launcher + SEA 入口 |

## 理由

空壳 core 无消费者；当前仅 HTTP 一个入口时，编排与传输同包合理（YAGNI）。

## 重新评估条件

出现第二个编排消费者（CLI / gRPC / 批处理）时再抽独立编排层。

## 相关

- knowledge-base：`ADR-0004-TS三层包结构.md`
