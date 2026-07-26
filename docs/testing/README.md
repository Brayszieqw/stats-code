# Stats Code · 测试与验证文档

本目录存放 **Stats Code** 的浏览器验收、数值对账、Codex Chrome/Browser 使用说明与可复制提示词。

| 文档 | 说明 |
|------|------|
| [codex-chrome-stats-code-qa.md](./codex-chrome-stats-code-qa.md) | **主文档**：分层验证体系、环境准备、Bug 清单、公开库 vs 官方统计协议、完整提示词库、日常流程与排障 |
| [prompts-codex-chrome.md](./prompts-codex-chrome.md) | **提示词速查**：可直接粘贴到 ChatGPT 桌面 App · Codex 的模板 |

## 快速入口

1. 启动 dev：`启动Stats前端.bat`（Vite `:5173` + API `:8080`）
2. 算法 parity：`cd ts-backend ; npm test -- tests/parity`
3. 脚本化 UI 烟测：`node web/scripts/full-ui-click-test.mjs`
4. Codex 浏览器：桌面 App → `@Browser` / `@Chrome`（见主文档）

## 相关仓库资产

- 演示数据：`web/public/demo_cohort.csv`、`demo_cohort_with_issues.csv`
- 数值 oracle：`ts-backend/tests/parity/known_values/`
- 历史 Chrome 审计：`work/chrome-audit.mjs`、`work/chrome-audit-20260711/`
- 领域与 parity 说明：`CONTEXT.md`、`knowledge-base/Parity与Sidecar.md`
