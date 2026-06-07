# ADR-0005：Power 家族对齐 SAS PROC POWER，刻意偏离 Rust normal-approximation

- **状态**：Accepted
- **日期**：2026-06-08
- **相关 Spec**：`.kiro/specs/typescript-backend-rewrite/`（task 11.1 / 11.2）

## 背景

TS 重写的核心不变量是「数值等价于 Rust 后端」（Requirement 1 / 2）。绝大多数算法据此实现：TS 输出对照 Rust 行为基准 + 录制的 SAS/SPSS 基线，在 1e-6/1e-9（非迭代）或 1e-4/1e-7（迭代）容差内通过。

**Power 家族（`power_single_arm` / `power_phase2` / `power_phase3`）是例外。** 调查发现：
- Rust `crates/stats-code/src/power.rs` 用 **normal-approximation** 公式（z 值闭式 + Wald 样本量）。
- 录制的 SAS 基线来自 **PROC POWER**，用 noncentral-t / Cohen's h（arcsine）/ 标准化效应量，并对样本量做整数搜索。
- 两者方法不同，结果差异远超 1e-6（如 phase3 每臂 n：normal=63 vs SAS=64；phase2 效应量：raw diff=0.20 vs Cohen's h=0.5158）。
- Coverage_Matrix 把 power 家族标为 **`recorded`（非 `live`）**，即按 Requirement 2.6 它不要求 live Reference_Software parity。

用户在 task 11.2 执行时明确选择：**让 TS power 对齐 SAS PROC POWER**，而非保持与 Rust 一致。

## 决策

**TS power 家族（`packages/engine/src/stats/power.ts`）实现 SAS PROC POWER 的方法**：
- `power_single_arm`：标准化效应量 `(p1-p0)/√(p0(1-p0))`，normal 近似样本量与功效。
- `power_phase2`：Cohen's h（arcsine 变换），normal 近似。
- `power_phase3`：标准化均值差 `|Δ|/σ`，功效用**非中心 t 分布**（新增 `noncentralTCdf`，AS 243 / Lenth 算法），样本量用从 normal 种子向上整数搜索至达到目标功效。

`effect_size` 与 `required_n` 精确复现 SAS；`achieved_power` 在 **1e-3** 容差内（PROC POWER 内部 quadrature 与本引擎 CDF 的方法差，无法到 1e-6）。

## 理由

1. 用户显式决策（两次确认），优先与终端用户熟悉的 SAS 输出一致。
2. power 是 `recorded` 而非 `live` 单元，Requirement 2.6 的 live-parity 强制不适用。
3. 临床/科研用户更可能用 SAS PROC POWER 交叉核对样本量，对齐它降低"为什么和 SAS 不一样"的支持成本。

## 后果

### 正面
- power 输出与 SAS PROC POWER 一致，`effect_size`/`required_n` 精确匹配（已有 parity 测试 `tests/parity/power.parity.test.ts` 实证）。
- 新增的 `noncentralTCdf` 是可复用的数学原语。

### 负面（重要）
- **这是整个 TS 重写中唯一刻意打破「TS↔Rust 数值等价」不变量的地方。** 任何对照 Rust power 输出的回归检查会"失败"——但这是预期，不是 bug。
- `achieved_power` 只能保证 1e-3，弱于其它算法的 1e-6/1e-9。
- power 单元测试（`tests/unit/stats-power.test.ts`）pin 的是 SAS 值，不是 Rust 值。

## 触发重新评估的条件
- 决定 TS↔Rust 严格等价高于 SAS 对齐 → 回退 power.ts 到 normal-approximation，并把 power 基线重录为 Rust 输出。
- SAS 更新 PROC POWER 算法导致基线漂移。
