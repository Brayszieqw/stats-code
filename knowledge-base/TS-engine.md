---
type: package
language: typescript
status: stable
path: ts-backend/packages/engine
tags: [typescript, stats-engine, pure-functions]
---

# TS-engine

> **职责**：纯计算层（依赖链底端）。17 个 Output-Level 统计算法 + 数学核心 + launcher 原语 + sidecar + snapshot + parity + coverage + spawn_policy。全部为**进程内纯函数**。

## 模块（`packages/engine/src/`）

| 模块 | 作用 |
|------|------|
| `stats/` | 17 个统计算法（进程内纯函数） |
| `math/` | 数学核心（log-gamma、不完全 beta/gamma、非中心 t…） |
| `launcher/` | 端口扫描 / 浏览器 / 进程原语 |
| `sidecar/` | 等价代码侧栏生成 |
| `snapshot/` | [[审计快照]] 导出 |
| `parity/` | parity 辅助 |
| `coverage/` | 算法覆盖矩阵 |
| `spawn_policy.ts` | 禁止 spawn 外部统计运行时 |
| `redact.ts` | 密钥 / 路径脱敏 |
| `version.ts` | 版本 / commit / 依赖快照 |
| `cli.ts` · `bin.ts` | 隐藏 CLI 分发契约 |

## 关键设计：进程内纯函数

见 [[ADR-0003-TS后端进程内算法]]。

- 算法 = 无副作用纯函数，由 [[TS-server]] 直接 import，**不 spawn 子进程**。
- `spawn_policy.ts` 哨兵阻断对 R/Rscript/python/sas/spss/pspp 等的 spawn。
- 数值正确性靠 `tests/parity` 与数学性质测试，不依赖本机安装参考软件。

## 相关

- [[统计引擎]] · [[Parity与Sidecar]] · [[审计快照]]
- [[ADR-0005-Power家族对齐SAS]]
