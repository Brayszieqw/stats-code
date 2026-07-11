# Stats Code 知识库 (Obsidian Vault)

这是 **Stats Code** 项目的代码知识库，用 Obsidian 风格的双向链接 Markdown 笔记组织。

> **当前真相**：生产后端仅为 **TypeScript**（`ts-backend/`，Node.js 22）。原 Rust workspace 已于 2026-06 退役删除；本库不再维护 `Rust-*` 节点，亦不再保留 `rust-final` 档案 tag。

## 怎么用

1. 打开 Obsidian → `Open folder as vault` → 选择这个 `knowledge-base/` 文件夹。
2. 打开 [[项目总览]] 作为入口（MOC, Map of Content）。
3. 点右上角 **Graph View**（图谱视图）查看模块关系。
4. 在每篇笔记的 frontmatter 里看 `status` 字段判断进度。

## 节点命名约定

- `TS-*` = TypeScript 后端（`ts-backend/packages/`）
- `Web-*` = 前端（`web/`）
- `ADR-*` = 架构决策记录
- 其余 = 概念 / 总览节点

## 状态图例

| status | 含义 |
|--------|------|
| `stable` | 已实现且为当前生产路径 |
| `in-progress` | 正在开发 |
| `scaffold` | 仅骨架 / 占位 |
| `planned` | 规划中，尚未动工 |
| `historical` | 历史决策/已退役说明，勿当现状 |

## 标签体系

- `#typescript` `#frontend` —— 语言/层
- `#llm` `#stats-engine` `#http` `#cli` —— 职责域
- `#adr` —— 架构决策

> 反映 `D:\stats code\repo` 的 **TS 时代**结构。代码变更后请同步更新对应笔记。
