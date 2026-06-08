---
type: adr
status: accepted
tags: [adr]
---

# ADR-0002:接受 sled 未维护的传递依赖

> 接受 `sled`(及其未维护的传递依赖)在 cargo-audit 中的告警,记录为已知可控风险,不阻塞构建。

## 背景
项目 `audit/` 与 `.cargo/audit.toml` 维护安全审计配置,某些传递依赖处于未维护状态。

## 决策
明确接受这些告警(在 audit 配置中忽略对应 advisory),作为权衡记录在案,而非静默忽略。

## 相关
- [[项目总览]]
- 涉及 `audit/`、`crates/` 的依赖配置
