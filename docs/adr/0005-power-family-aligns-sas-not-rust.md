# ADR-0005：Power 家族对齐 SAS PROC POWER

**状态**：accepted  
**文件名保留 historical 后缀**：曾对比「normal-approximation」基线；**当前权威是 SAS**，不是任何已删除的旧实现。

## 决策

`power_single_arm` / `power_phase2` / `power_phase3` 对齐 **SAS PROC POWER**（含 noncentral-t、Cohen's h、整数搜索等模块约定）。Coverage 标为 `recorded`；单元测试 pin SAS 录制值。

## 后果

- 与「纯正态近似」基线不一致是**预期**
- Oracle：`tests/parity/known_values/sas/`

## 相关

- knowledge-base：`ADR-0005-Power家族对齐SAS.md`
- `packages/engine/src/stats/power.ts`
