# ADR-0003：TS 后端用进程内纯函数实现算法

**状态**：accepted  
**日期**：2026-06（TS 重写期）

## 决策

TS 后端把统计算法实现为 `packages/engine` 内的**进程内纯函数**，HTTP/skill 层直接调用，**不 spawn** 外部统计运行时。`spawn_policy` 主动阻断对 R / python / sas / spss / pspp 等的 spawn。

## 理由

1. 单文件零依赖产物（Node SEA）
2. 纯函数便于 parity 与属性测试
3. 审计上可证明未调用未声明软件
4. 避免子进程 JSON 往返开销

## 后果

- 数学核心在 TS 内实现；正确性依赖 `tests/parity` 与 SAS/SPSS 录制基线
- 浏览器打开等合法 spawn 须在哨兵作用域之外

## 相关

- knowledge-base：`ADR-0003-TS后端进程内算法.md`
- `CONTEXT.md`、`packages/engine/src/spawn_policy.ts`
