# ADR-0002：接受 sled 引入的 unmaintained 传递依赖（fxhash / instant）

- **状态**：Accepted
- **日期**：2026-06-06
- **相关 Spec**：`.kiro/specs/engineering-quality-hardening/`（R5）

## 背景

`agent-server` 的持久化会话存储 `SledSessionStore`（实现 `agent-core` 的
`SessionStore` trait，见 `crates/agent-core/src/store/sled_session_store.rs`）
基于嵌入式 KV 引擎 [`sled` 0.34.7](https://crates.io/crates/sled)。它在
`agent-server` 以 `--session-store sled:<path>` 启动时提供跨进程重启的持久会话，
是 `MemSessionStore` 的可选耐久替代。

`cargo audit` 在 CI 中对 `sled` 的依赖子树报告了两条 **unmaintained**（非漏洞）
告警：

| Crate | 版本 | Advisory | 类型 | 依赖路径 |
|-------|------|----------|------|----------|
| `fxhash` | 0.2.1 | [RUSTSEC-2025-0057](https://rustsec.org/advisories/RUSTSEC-2025-0057) | unmaintained | `sled → fxhash` |
| `instant` | 0.1.13 | [RUSTSEC-2024-0384](https://rustsec.org/advisories/RUSTSEC-2024-0384) | unmaintained | `sled → parking_lot 0.11 → parking_lot_core → instant` |

两条都是 **unmaintained**（上游不再维护），**不是已知漏洞**（no known
vulnerability）。它们由 `sled 0.34.x` 的依赖树间接引入，**不在本项目可直接控制
的依赖项里**——除非更换或升级 `sled` 本身。

需求阶段考虑了三种处理方式：

- **A. 迁移到 `redb`**：用纯 Rust、活跃维护的 `redb` 替换 `sled`，从根上消除这两条
  传递依赖。
- **B. 接受现状 + 显式 ignore 告警**：保留 `sled`，在 `.cargo/audit.toml` 的
  `[advisories].ignore` 中显式登记这两条 advisory，并以本 ADR 记录决策依据与
  重评触发条件。
- **C. 不处理**：放任 `cargo audit` 在 CI 中以非零码失败，或全局放宽 audit 策略。

## 决策

**采用方案 B**：保留 `sled` 作为 `SledSessionStore` 的后端，将
`RUSTSEC-2025-0057`（fxhash）与 `RUSTSEC-2024-0384`（instant）显式登记到
`.cargo/audit.toml` 的 `[advisories].ignore` 列表，每条以注释指向本 ADR。

理由：

1. **两条都是 unmaintained，不是漏洞**。无已知可利用的安全问题，风险等级低。
2. **`SledSessionStore` 是可选路径**：默认会话存储是 `MemSessionStore`；
   `sled` 仅在用户显式选择持久化时启用，攻击面有限。
3. **`redb` 迁移成本 > 当前收益**：迁移需要重写 `SessionStore` 的持久化实现、
   迁移既有 on-disk 数据格式、补齐回归与 parity，属于独立的工程项，不应塞进
   本次「质量硬化」spec 的范围（YAGNI / 范围控制）。
4. **决策可逆**：ignore 是声明式、最小、可随时撤销的；未来迁移 `redb` 或 `sled`
   升级后，删掉两行 ignore 即可恢复严格审计。

## 备选方案及驳回理由

### 方案 A（迁移到 `redb`）

被推迟（非永久驳回）：

1. **范围**：本 spec（engineering-quality-hardening）的目标是 lint/dead-code/
   测试硬化，不是存储引擎迁移。把一次后端替换混入会放大 blast radius。
2. **数据迁移**：已有用户的 `sled:<path>` 目录是 sled 私有格式，迁移到 `redb`
   需要一次性导出/导入逻辑，且要测试跨版本兼容。
3. **回归风险**：`SessionStore` 是 `agent-server` 的核心耐久路径，替换需要完整的
   持久化/并发回归，独立成项更稳。

`redb` 迁移留待未来独立 spec，触发条件见下。

### 方案 C（不处理）

被驳回：会让 `cargo audit` 在 CI 中持续以非零码失败（或迫使全局放宽审计策略），
既掩盖未来可能出现的**真实漏洞**告警，又丢失对这两条已知 unmaintained 项的
显式问责记录。显式 ignore + ADR 比"全局静音"安全得多。

## 后果

### 正面

- **CI 审计恢复绿色**：`cargo audit` 退出 0，同时仍对**其他**未登记的告警敏感
  （只静音这两条具名 advisory，不放宽全局策略）。
- **决策可追溯**：每条 ignore 注释指向本 ADR，未来维护者能立刻看懂为什么静音。
- **范围受控**：不把存储迁移塞进质量硬化 spec。
- **可逆**：迁移或升级后删两行即可恢复严格审计。

### 负面

- **暂时背负两条 unmaintained 传递依赖**：若上游将来发现真实漏洞（从
  unmaintained 升级为 vulnerability），需要尽快推进 `redb` 迁移或 `sled` 升级。
- **ignore 列表需要定期复核**：避免"静音即遗忘"，复核节奏由下方触发条件约束。

## 触发重新评估的条件

满足任意一条即重新审视本决策（并优先考虑方案 A 的 `redb` 迁移）：

- `fxhash` 或 `instant` 的 advisory 从 **unmaintained 升级为已知漏洞**
  （RUSTSEC 类型变为 vulnerability）。
- `sled` 发布去除了 `fxhash` / 老版 `parking_lot` 依赖的新版本，可通过升级消除告警。
- `SledSessionStore` 从可选路径变为默认/必选的持久化后端，攻击面上升。
- 出现对持久化存储的新需求（如多进程并发、事务、加密），使 `redb` 迁移本身
  具备独立收益。
