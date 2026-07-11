---
type: concept
status: stable
tags: [parity, sidecar, concept, typescript]
spec: .kiro/specs/parity-and-multilang-sidecar
---

# Parity 与 Sidecar

> 横切概念：保证统计算法输出与 **参考软件**（SAS / SPSS / 教科书已知值）在容差内一致，并在前端展示「等价代码侧栏」（R / SAS / Python / SPSS）。

## 三个子系统

### 1. Coverage Matrix（覆盖矩阵）

parity 覆盖的单一真相源。每个算法标 `live` 或 `recorded`。

- Engine：[[TS-engine]] `coverage/`
- 前端：[[Web-前端]] `lib/coverageMatrix.ts`

### 2. Sidecar（等价代码侧栏）

每个统计结果附带多语言等价代码片段，带脱敏（`redact`）与版本页脚。

- Engine：[[TS-engine]] `sidecar/`
- 前端：`EquivalentCodeSidecar.tsx` / `SidecarFooter.tsx`

### 3. Parity 验证

- 套件：`ts-backend/tests/parity/`
- 基线：`known_values/`（含 `sas/`、`spss/`）
- 典型容差：非迭代约 1e-6；迭代约 1e-4 量级（以测试配置为准）

## Spawn Policy 哨兵

禁止 spawn 外部统计运行时（R / python / sas / spss / pspp 等），保证审计可信。见 [[ADR-0003-TS后端进程内算法]]。

## 唯一主动偏离：Power 家族

Power 算法对齐 **SAS PROC POWER**，而非历史 normal-approximation 基线。见 [[ADR-0005-Power家族对齐SAS]]。

## 相关

- [[统计引擎]] · [[审计快照]] · [[TS-engine]]
