---
type: adr
status: accepted
tags: [adr, typescript]
---

# ADR-0003:TS 后端用进程内纯函数实现算法

> TS 后端把 17 个算法实现为**进程内纯函数**(`packages/engine/src/stats/*.ts`),HTTP 层直接调用,**不 spawn 子进程**。引入 Spawn_Policy 哨兵主动阻断对外部统计运行时(R/python/sas/spss/pspp)的 spawn。

## 与 Rust 版的对比
- **Rust**:算法 = 子进程,[[Rust-agent-core]] 通过 `SkillInvoker::StatsCli` spawn `stats-code --json`。
- **TS**:算法 = 进程内纯函数 + 禁止外部运行时 spawn。模型被**反转**。

## 理由
1. **单文件零依赖产物**:Node SEA 内嵌运行时,目标机不需装 Node/R/SAS/Python。
2. **确定性与可测性**:纯函数便于 parity 测试和属性测试(PBT)。
3. **审计可信度**:禁 spawn 可证明"没偷调未声明的统计软件"。
4. **性能**:免去进程创建 + JSON 往返。

## 后果
- 算法必须用 TS 重写,数学核心需自己实现并对照 Rust 验证(parity 风险主来源)。
- 浏览器拉起必须放在哨兵作用域**之外**,否则被误杀。
- `CONTEXT.md` 里"子进程调用"描述只适用于 Rust 版。

## 相关
- [[TS-engine]] · [[TS-server]] · [[Parity与Sidecar]] · [[TS后端重写]]
