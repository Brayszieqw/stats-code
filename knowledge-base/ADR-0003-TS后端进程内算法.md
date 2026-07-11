---
type: adr
status: accepted
tags: [adr, typescript]
---

# ADR-0003: TS 后端用进程内纯函数实现算法

> TS 后端把 17 个算法实现为**进程内纯函数**（`packages/engine/src/stats/*.ts`），HTTP 层直接调用，**不 spawn 子进程**。引入 Spawn_Policy 哨兵主动阻断对外部统计运行时（R/python/sas/spss/pspp）的 spawn。

## 决策要点

- 算法 = 进程内纯函数 + 禁止外部统计运行时 spawn。
- 目标机不需安装 Node / R / SAS / Python（Node SEA 内嵌运行时）。

## 理由

1. **单文件零依赖产物**
2. **确定性与可测性**：便于 parity 与属性测试
3. **审计可信度**：可证明未调用未声明的统计软件
4. **性能**：免进程创建与 JSON 往返

## 后果

- 数学核心需在 TS 内实现，正确性依赖 `tests/parity` 与参考软件录制基线。
- 浏览器拉起等允许的 spawn 必须放在哨兵作用域之外。

## 相关

- [[TS-engine]] · [[TS-server]] · [[Parity与Sidecar]] · [[TS后端重写]]
