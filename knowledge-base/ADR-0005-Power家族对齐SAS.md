---
type: adr
status: accepted
tags: [adr, typescript, stats-engine]
---

# ADR-0005: Power 家族对齐 SAS PROC POWER

> TS power 家族（`power_single_arm` / `power_phase2` / `power_phase3`）实现 **SAS PROC POWER** 方法。这是相对「普通正态近似样本量」的主动选择，单测 pin 的是 SAS 录制值。

## 背景

- 正态近似（z 闭式 + Wald）与 SAS（noncentral-t / Cohen's h / 整数搜索）在边界上可差整例 n。
- Coverage Matrix 将 power 标为 `recorded`（非强制 live 外部软件）。

## 决策

TS power 对齐 SAS：

- `power_single_arm`：标准化效应量 + normal 近似（按 SAS 模块约定）
- `power_phase2`：Cohen's h（arcsine）
- `power_phase3`：标准化均值差 + **非中心 t** + 整数搜索

`effect_size` / `required_n` 精确复现 SAS；`achieved_power` 容差约 1e-3。

## 后果

- 与「纯正态近似」基线对照会不一致——这是**预期**，不是回归。
- 权威 oracle：`tests/parity/known_values/sas/`。

## 相关

- [[TS-engine]] · [[统计引擎]] · [[Parity与Sidecar]]
