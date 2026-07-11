---
type: package
language: typescript
status: stable
path: web
tags: [frontend, react, vite]
---

# Web-前端

> **职责**：用户唯一面对的界面。React 19 + Vite + Ant Design 的统计分析平台，对接 [[TS-server]]（`/api/*`）。

## 入口

`main.tsx` → `App.tsx` → Simple / Pro 视图（`views/`）。

## 关键组件（`src/components/`）

| 组件 | 作用 |
|------|------|
| `OnboardingCard.tsx` | 首次 LLM Key 配置遮罩 |
| `ConnectionBanner.tsx` / `ErrorBanner.tsx` | LLM 异常横幅 |
| `MessageList.tsx` | 消息流 |
| `DatasetUploader.tsx` / `DataExplorer.tsx` | 数据上传与浏览 |
| `AnalysisConfigurator.tsx` | 分析配置 |
| `AnalysisResultView.tsx` | 结果展示 |
| `ThreeLineTable.tsx` | 三线表 |
| `StatsChartRenderer.tsx` | 统计图表 |
| `EquivalentCodeSidecar.tsx` / `SidecarFooter.tsx` | 等价代码侧栏 |
| `ExportSnapshotButton.tsx` | 导出 [[审计快照]] |
| `ChoicePromptCard.tsx` | 选项卡片 |
| `VoiceRecorder.tsx` | 语音输入 |

## Hooks（`src/hooks/`）

- `useSseChat.ts` —— SSE 流式聊天
- `useLlmStatus.ts` —— LLM 配置状态
- `useDatasetUpload.ts` · `useSidecar.ts` · `useSnapshotExport.ts` · `useSessionController.ts` 等

## 相关

- [[TS-server]] · [[Parity与Sidecar]] · [[LLM集成]] · [[单命令启动器]]
