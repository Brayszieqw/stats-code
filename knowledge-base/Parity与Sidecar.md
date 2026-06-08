---
type: concept
status: in-progress
tags: [parity, sidecar, concept]
spec: .kiro/specs/parity-and-multilang-sidecar
---

# Parity 与 Sidecar

> 横切概念:保证 TS 算法输出与 Rust/参考软件(SAS/SPSS)数值等价,并在前端展示"等价代码侧栏"。

## 三个子系统

### 1. Coverage Matrix(覆盖矩阵)
parity 覆盖的单一真相源。每个算法标 `live`(要求实时参考软件 parity)或 `recorded`(用录制基线)。
- Rust:[[Rust-stats-code]] `coverage_matrix/` + 内嵌 `matrix.toml`
- TS:[[TS-engine]] `coverage/`
- 前端:[[Web-前端]] `lib/coverageMatrix.ts`

### 2. Sidecar(等价代码侧栏)
在前端展示每个统计结果对应的等价代码片段,带脱敏(`redact`)和版本页脚(`SidecarFooter`)。
- Rust `sidecar/` · TS `sidecar/` · 前端 `EquivalentCodeSidecar.tsx`

### 3. Parity 验证
对照容差(`validation/tolerance_config.yaml`)逐算法验证数值等价。
- 非迭代算法:1e-6/1e-9;迭代算法:1e-4/1e-7
- TS 测试:`tests/parity/`

## Spawn Policy 哨兵
禁止 spawn 外部统计运行时(R/python/sas/spss/pspp),保证审计可信。见 [[ADR-0003-TS后端进程内算法]]。

## 唯一等价例外:Power 家族
Power 算法刻意对齐 SAS PROC POWER 而非 Rust。见 [[ADR-0005-Power家族对齐SAS]]。

## 相关
- [[统计引擎]] · [[审计快照]] · [[TS-engine]]
