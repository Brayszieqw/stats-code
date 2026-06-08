---
type: adr
status: accepted
tags: [adr, typescript, stats-engine]
---

# ADR-0005:Power 家族对齐 SAS PROC POWER

> TS power 家族(`power_single_arm` / `power_phase2` / `power_phase3`)实现 SAS PROC POWER 的方法,**刻意偏离** Rust 的 normal-approximation。这是整个 TS 重写中**唯一**主动打破"TS↔Rust 数值等价"的地方。

## 背景
- Rust `power.rs` 用 normal 近似(z 闭式 + Wald 样本量)。
- 录制的 SAS 基线用 noncentral-t / Cohen's h / 标准化效应量 + 整数搜索。
- 两者差异远超 1e-6(如 phase3 每臂 n:normal=63 vs SAS=64)。
- Coverage Matrix 把 power 标为 `recorded`(非 `live`),不强制 live parity。

## 决策
TS power 对齐 SAS:
- `power_single_arm`:标准化效应量 + normal 近似
- `power_phase2`:Cohen's h(arcsine)
- `power_phase3`:标准化均值差 + **非中心 t 分布**(新增 `noncentralTCdf`)+ 整数搜索

`effect_size`/`required_n` 精确复现 SAS;`achieved_power` 仅 1e-3 容差。

## 后果
- ⚠ 对照 Rust power 的回归检查会"失败",这是**预期**不是 bug。
- power 单测 pin 的是 SAS 值,不是 Rust 值。

## 相关
- [[TS-engine]] · [[统计引擎]] · [[Parity与Sidecar]] · [[TS后端重写]]
