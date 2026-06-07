# ADR-0004：TypeScript 后端采用三层包结构，移除空壳 `core` 包

- **状态**：Accepted
- **日期**：2026-06-08
- **相关 Spec**：`.kiro/specs/typescript-backend-rewrite/`

## 背景

TS 重写最初按 Rust 的四个 crate 1:1 映射出四个包：

| Rust crate | 职责 | 初始 TS 包 |
|---|---|---|
| `api` | 顶层组合 | `packages/api` |
| `agent-core` | 会话/消息流/Skill 注册表/编排 | `packages/core` |
| `agent-server` | HTTP 服务 | `packages/server` |
| `stats-code` | 算法引擎 | `packages/engine` |

依赖方向用 `eslint-plugin-import` 强制为 `api → core → server → engine`。

实现完成后发现：**`packages/core` 始终是空壳**——`core/src/index.ts` 只导出一个常量 `CORE_PACKAGE`，没有任何实质逻辑，也没有任何包 import 它的内容。Rust `agent-core` 的职责（SessionStore、LLM 配置/探测、消息编排 MessageHandler/AgentEvent、SSE 帧）在 TS 版**全部落在了 `server` 包内**（`state.ts`、`mem-store.ts`、`llm.ts`、`sse.ts`）。

对 `core` 做 Deletion test：删除它，复杂度消失、无调用方受影响 → 它是 pass-through 空模块，没有"挣得自己的位置"。

## 决策

**移除 `packages/core`，TS 后端采用三层包结构 `api → server → engine`。**

- `engine`：纯计算（算法、数学核心、launcher 原语、sidecar、snapshot、replay、parity、coverage、spawn_policy）。
- `server`：HTTP 传输 **+ 编排**（Fastify 路由、契约校验、SSE、SPA 兜底、SessionStore、LLM 配置/探测、provider 接线）。
- `api`：应用组合（launcher runner + SEA bin 入口）。

eslint 边界规则、根 `package.json` workspaces、根/各包 `tsconfig.json` references 同步改为三层。

## 理由

1. **结构应反映现实**（Deletion test）：空壳包是"假 seam"，给人有编排层的错觉，实际没有。删掉让包结构与真实模块边界一致。
2. **YAGNI**：把编排从 server 拆进 core 只在存在**第二个非 HTTP 入口**（如独立 CLI）需要复用编排时才有杠杆。当前唯一入口是 HTTP server，编排与传输同包是合理的局部性安排。
3. **降低误导**：维护者不会再去空的 core 包找"会话/消息编排"代码。

## 备选方案及驳回理由

### 候选 A：充实 `core`，把编排从 server 下沉进去（恢复 agent-core 语义）
驳回（当前）：需要**反转 core↔server 依赖方向**（Rust 里 core 依赖 server 是反常的；正确应是 server 依赖 core），是一次真正的架构变更。在没有第二个编排消费者的前提下，收益不足以抵消改动面。**若未来新增非 HTTP 入口（CLI / gRPC / 批处理），应重开此候选**——届时编排需要被独立复用，core 才挣得位置。

### 候选 B：保持四包不动
驳回：留着空壳违背"结构反映现实"，且持续误导。

## 后果

### 正面
- 包结构诚实：三层各有明确深度，无 pass-through 空模块。
- 依赖图更简单，eslint 边界规则少一条。
- 删除是低风险纯移动：build / typecheck / lint / 343 测试全绿（已实证）。

### 负面
- 放弃与 Rust 4-crate 的 1:1 对应。但 ADR-0003 已说明 TS 版范式（进程内算法）本就偏离 Rust，1:1 对应不再是目标。
- `server` 包承载两类关注点（传输 + 编排）。若日后 server 膨胀到难以导航，触发条件见下。

## 触发重新评估的条件
- 新增第二个编排消费者（非 HTTP 入口）→ 重开候选 A，把编排抽进独立包。
- `server` 包文件数/认知复杂度增长到传输与编排互相干扰、难以单独测试。
