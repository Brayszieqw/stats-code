---
type: package
language: typescript
status: stable
path: web
tags: [frontend, react, vite]
---

# Web-前端

> **职责**:用户唯一面对的界面。React 19 + Vite + Ant Design + ECharts 的聊天式统计分析平台。两套后端([[Rust-agent-server]] / [[TS-server]])共用,基本不变。

## 入口
`main.tsx` → `App.tsx`,路由到页面。后端通过 `/api/*` 对接。

## 页面 (`src/pages/`)
- `ChatPage.tsx` —— 主聊天界面(对话式分析)
- `WorkflowPage.tsx` —— 工作流页面

## 关键组件 (`src/components/`)

| 组件 | 作用 |
|------|------|
| `OnboardingCard.tsx` | 首次运行 LLM Key 配置卡片(未配置时遮罩) |
| `ConnectionBanner.tsx` / `ErrorBanner.tsx` | LLM 连接异常横幅(重试/改 Key) |
| `MessageList.tsx` | 消息流渲染 |
| `DatasetUploader.tsx` / `DataExplorer.tsx` | 数据上传与浏览 |
| `AnalysisConfigurator.tsx` | 分析配置 |
| `AnalysisResultView.tsx` | 结果展示 |
| `ThreeLineTable.tsx` | 三线表(医学论文标准表格) |
| `StatsChartRenderer.tsx` | 统计图表(ECharts) |
| `EquivalentCodeSidecar.tsx` / `SidecarFooter.tsx` | 等价代码侧栏(见 [[Parity与Sidecar]]) |
| `ExportSnapshotButton.tsx` | 导出[[审计快照]] |
| `ChoicePromptCard.tsx` | LLM 选项卡片 |
| `VoiceRecorder.tsx` | 语音输入 |

## Hooks (`src/hooks/`)
- `useSseChat.ts` —— SSE 流式聊天
- `useLlmStatus.ts` —— LLM 配置状态
- `useDatasetUpload.ts` · `useSidecar.ts` · `useSnapshotExport.ts` · `useConnectionBanner.ts`

## 库 (`src/lib/`)
- `coverageMatrix.ts` + context —— 算法覆盖矩阵
- `bannerReducer.ts` · `analysisResultMount.ts`

## 运行模式
- **dev**:vite `:5173`,代理 `/api/*` 到 `:8080`
- **prod**:由后端从内嵌 `web/dist/` 直接吐静态资源

## 相关
- [[领域语言]] · [[单命令启动器]]
- [[Rust-agent-server]] · [[TS-server]]
