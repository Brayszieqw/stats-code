---
type: package
language: typescript
status: in-progress
path: ts-backend/packages/engine
tags: [typescript, stats-engine, pure-functions]
---

# TS-engine

> **职责**:纯计算层(依赖链底端)。17 个 Output-Level 统计算法 + 数学核心 + launcher 原语 + sidecar + snapshot + parity + coverage + spawn_policy。全部为**进程内纯函数**。

## 模块 (`packages/engine/src/`)

| 模块 | 作用 | Rust 对应 |
|------|------|-----------|
| `stats/` | 17 个统计算法(进程内纯函数) | [[Rust-stats-code]] `stats/` + 各算法 .rs |
| `math/` | 数学核心(log-gamma、不完全 beta/gamma、非中心 t…) | stats-code `math/` |
| `launcher/` | 端口扫描/浏览器/进程原语 | stats-code `launcher/` |
| `sidecar/` | 等价代码侧栏生成 | stats-code `sidecar/` |
| `snapshot/` | [[审计快照]]导出 | stats-code `snapshot/` |
| `parity/` | parity 验证 | stats-code `parity/` |
| `coverage/` | 算法覆盖矩阵 | stats-code `coverage_matrix/` |
| `spawn_policy.ts` | 禁止 spawn 外部统计运行时哨兵 | stats-code `spawn_policy.rs` |
| `redact.ts` | 密钥/路径脱敏 | stats-code `redact.rs` |
| `version.ts` | 版本/commit/依赖快照 | stats-code `build.rs` 注入 |
| `cli.ts` · `bin.ts` | CLI 入口 | stats-code `cli.rs` `main.rs` |

## 关键设计:进程内纯函数(见 [[ADR-0003-TS后端进程内算法]])

- 算法 = 无副作用纯函数,由 [[TS-server]] 直接 import 调用,**不 spawn 子进程**。
- `spawn_policy.ts` 哨兵给 `child_process` 打补丁,**主动阻断**对 R/Rscript/python/sas/spss/pspp 的 spawn,命中即抛 `ForbiddenSpawnError`。这是单文件零依赖产物 + 审计可信度的基础。
- 数学核心需自己写连分式/级数实现并对照 Rust 验证 —— parity 风险主要来源。

## 相关
- [[统计引擎]] · [[Parity与Sidecar]] · [[审计快照]]
- [[ADR-0005-Power家族对齐SAS]](本层算法的唯一等价例外)
