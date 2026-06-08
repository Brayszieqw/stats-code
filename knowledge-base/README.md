# Stats Code 知识库 (Obsidian Vault)

这是 **Stats Code** 项目的代码知识库,用 Obsidian 风格的双向链接 Markdown 笔记组织。

## 怎么用

1. 打开 Obsidian → `Open folder as vault` → 选择这个 `knowledge-base/` 文件夹。
2. 打开 [[项目总览]] 作为入口(MOC,Map of Content)。
3. 点右上角 **Graph View**(图谱视图),就能看到整个项目的"版图":
   - 每个节点 = 一个模块 / crate / 子系统
   - 连线 = 依赖、调用、实现关系
4. 在每篇笔记的 frontmatter 里看 `status` 字段判断进度。

## 节点命名约定

- `Rust-*` = Rust crate(`crates/` 下)
- `TS-*` = TypeScript 后端(`ts-backend/` 下)
- `Web-*` = 前端(`web/` 下)
- `ADR-*` = 架构决策记录
- 其余 = 概念 / 总览节点

## 状态图例

| status | 含义 |
|--------|------|
| `stable` | 已实现且成熟(Rust 版主体) |
| `in-progress` | 正在开发 / 重写中 |
| `scaffold` | 仅骨架 / 占位 |
| `planned` | 规划中,尚未动工 |

## 标签体系

- `#rust` `#typescript` `#frontend` —— 语言/层
- `#llm` `#stats-engine` `#http` `#cli` —— 职责域
- `#adr` —— 架构决策

> 这份知识库由对仓库的静态分析生成,反映 `d:\stats code\repo` 的当前结构。代码变更后可重新生成或手动补充。
