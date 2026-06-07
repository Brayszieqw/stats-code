# ADR-0003：TypeScript 后端用进程内纯函数实现算法，废弃子进程调用模型

- **状态**：Accepted
- **日期**：2026-06-08
- **相关 Spec**：`.kiro/specs/typescript-backend-rewrite/`

## 背景

Rust 版架构（见 `CONTEXT.md` §角色）里，算法引擎是 `crates/stats-code` 内部的 `stats/` 模块，由 `agent-core` 通过 **spawn `stats-code <subcommand> --json` 子进程**调用：

> Agent Core 负责会话、消息流、Skill 注册表，把统计请求路由到 stats engine……stats engine 被 `agent-core` 通过 `SkillInvoker::StatsCli` 子进程调用。

TypeScript 重写（`ts-backend/`）需要决定：是延续"后端 spawn 算法子进程"的模型，还是把算法直接作为进程内函数调用。

## 决策

**TS 后端把 17 个 Output-Level 算法实现为进程内纯函数**（`packages/engine/src/stats/*.ts`、`math/*.ts`），由 HTTP 层直接 import 调用，**不 spawn 任何子进程**。

更进一步，TS 后端引入了一个 **Spawn_Policy 哨兵**（`packages/engine/src/spawn_policy.ts`）：在 sidecar 渲染、快照导出、SPA 服务等受控管线内，对 `child_process.{spawn,exec,execFile,...}` 打补丁，**主动阻断**任何对外部统计运行时（R / Rscript / python / sas / spss / pspp 及对应共享库）的 spawn，命中即抛 `ForbiddenSpawnError`。

即：Rust 版"算法 = 子进程"的模型在 TS 版被**反转**为"算法 = 进程内纯函数 + 禁止外部运行时 spawn"。

## 理由

1. **单文件零依赖产物**（Requirement 10 / ADR-0001 的延续）：`stats-code.exe` 通过 Node SEA 内嵌运行时和前端资源，目标机器**不需要装 Node/R/SAS/Python/SPSS**。若延续子进程模型，算法子进程要么也得打包进 exe（复杂），要么依赖外部运行时（违背零依赖）。进程内纯函数天然契合 SEA。
2. **确定性与可测性**：纯函数（无时钟/环境/随机/IO）使 parity 测试和属性测试（PBT）可以直接对函数断言，无需起子进程、无需 mock stdout 解析。
3. **审计可信度**（Requirement 8）：禁止 spawn 外部统计运行时，使快照/sidecar 管线可证明"没有偷偷调用未声明的统计软件"，这是审计快照可信性的基础。
4. **性能**：免去每次算法调用的进程创建 + JSON 序列化往返开销。

## 后果

### 正面
- 产物零外部运行时依赖，冒烟测试在剥离 PATH 下仍能启动并服务（已实证）。
- 算法可被 17 个纯函数单测 + 5 个 parity 套件 + 多个 PBT 直接覆盖。
- spawn 哨兵提供纵深防御，外部统计运行时被结构化错误阻断。

### 负面
- 算法必须用 TS 重新实现（不能复用 R/SAS 包），数值核心（log-gamma、不完全 beta/gamma、非中心 t 等）需自己写连分式/级数并对照 Rust 验证。这是 parity 风险的主要来源，已用 `math-core` golden 测试和对照 SAS/SPSS 基线的 parity 套件覆盖。
- 浏览器拉起（launcher 打开默认浏览器）必须显式放在哨兵作用域**之外**（Requirement 8.5），否则会被误杀。

### 文档同步
- `CONTEXT.md` §角色 / §运行模式中"stats engine 被 agent-core spawn 子进程调用"的描述属于 Rust 版语义，对 TS 后端**已不适用**。本 ADR 即为该差异的权威记录；阅读 TS 代码时以本 ADR 为准。

## 触发重新评估的条件
- 出现必须复用某个只有 R/SAS/Python 实现、且 TS 无法在可接受成本内重写的算法。
- 产物允许依赖外部运行时（放弃单文件零依赖目标）。
