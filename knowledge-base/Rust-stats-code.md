---
type: crate
language: rust
status: stable
path: crates/stats-code
tags: [rust, stats-engine, cli, core]
---

# Rust-stats-code

> **职责**:项目的**心脏**。纯 Rust 统计引擎 + `stats-code` 二进制本体。包含所有统计算法、CLI 子命令、报告生成、审计快照、等价代码侧栏。零外部运行时依赖(不调 R/Python/SAS)。

## 在版图中的位置

基础层。被 [[Rust-agent-core]] 通过 `SkillInvoker::StatsCli` 以子进程方式调用(`stats-code <subcommand> --json`)。也是 [[单命令启动器]] 的二进制载体。

## 模块结构 (`crates/stats-code/src/`)

### 统计算法(见 [[统计引擎]])
| 模块 | 算法 |
|------|------|
| `logistic.rs` | Logistic 回归(Newton-Raphson IRLS) |
| `cox.rs` | Cox 比例风险模型(Efron/Breslow) |
| `linear.rs` | 线性回归(OLS / QR) |
| `survival.rs` | 生存分析(Kaplan-Meier 等) |
| `tableone.rs` | Table 1 基线特征表 |
| `power.rs` | 功效/样本量(对齐 SAS,见 [[ADR-0005-Power家族对齐SAS]]) |
| `rate.rs` | 率相关分析 |
| `modeling.rs` | 建模通用逻辑 |
| `stats/` | 更多统计方法集合(卡方、ANOVA、相关、PSM…) |
| `math/` | 底层数学函数(分布 CDF、矩阵运算) |

### 基础设施
| 模块 | 作用 |
|------|------|
| `cli.rs` | clap 命令定义(`tableone`/`survival`/`ttest`/`power`/`workflow`/`init`/`doctor`…) |
| `handlers.rs` | 子命令分派 `dispatch` / `run` |
| `report/` · `render/` | 报告构建与渲染 |
| `schema/` | 分析规格(`analysis.yaml`)与结果类型 |
| `launcher/` | [[单命令启动器]]:端口扫描、浏览器、进程守卫 |
| `sidecar/` · `snapshot/` · `parity/` | [[Parity与Sidecar]] + [[审计快照]] |
| `coverage_matrix/` | 算法覆盖矩阵(parity 单一真相源) |
| `spawn_policy.rs` | 哨兵:禁止 spawn 外部统计运行时(R/Python/SAS) |
| `redact.rs` | 密钥/路径脱敏策略(sidecar 与 snapshot 共用) |
| `bridge.rs` | `Engine` 对外门面 |
| `build.rs` | 编译期注入版本号、commit SHA、依赖快照 |

## 编译期常量(`lib.rs`)
- `RELEASE_VERSION` —— 来自 `CARGO_PKG_VERSION`
- `COMMIT_SHA` —— 来自 `git rev-parse HEAD`
- `RUNTIME_DEPS_JSON` —— 直接依赖版本快照

## 内部 CLI 子命令(对用户不可见)
`tableone`、`survival`、`ttest`、`power`、`workflow`、`init`、`doctor` 等。**不是公开用法**,仅供 [[Rust-agent-core]] 内部子进程调用。

## 规模
~40000 行,30+ 命令,157+ 测试,Clippy pedantic 零 warning。已通过 Python/R 数值一致性验证(`validation/`)。

## 已知技术债
`handlers.rs`(~3420 行)、`report.rs`(~3748 行)是 God File,待拆分(见 [[发展路线图]] T1)。

## 相关
- [[统计引擎]] · [[TS-engine]](TS 对应层)
- [[Parity与Sidecar]] · [[审计快照]]
