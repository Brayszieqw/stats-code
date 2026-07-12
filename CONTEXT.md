# Stats Code · 领域语言

> 共享词汇表。当代码里出现这些词，按这里的定义理解；当用户口头使用这些词，按这里的定义对齐。
>
> 后端为 **TypeScript（Node.js 22，`ts-backend/`）**。原 Rust workspace 已于
> 2026-06 退役并从仓库移除；Rust 时代分支/标签/工具链已清理，不再保留档案 tag。

## 产品命令

### `stats-code`（命令）
全局可执行的二进制（`stats-code.exe`，Node SEA 单文件，放在 PATH 里）。

**对用户暴露的行为**：

| 入口 | 行为 |
|------|------|
| **`desktop/` / `scripts/start-desktop.ps1`（推荐 GUI）** | Electron **应用内窗口**加载本机 SPA；后端以 `--no-browser` 启动，**不依赖系统浏览器** |
| `stats-code`（不带子命令） | 启动后端 + 内嵌 SPA，并打开**系统默认浏览器**（兼容旧路径） |
| `stats-code --no-browser` | 仅后端（供桌面壳或其他宿主连接） |

`stats-code` 公共旗标：`--version`、`--help`、`--no-browser`。**没有公开子命令。**

### 内部算法子命令
`tableone`、`survival`、`ttest`、`power` 等算法入口——
**不是公开 CLI 用法**。17 个算法是 `packages/engine` 内的**进程内纯函数**
（ADR-0003），由 HTTP 层直接调用，不 spawn 子进程；`engine/src/cli.ts`
保留了与旧 CLI 一致的隐藏分发契约。

## 部署形态

**本地单机模式**（localhost 应用，类似 OpenCode / Jupyter Lab / Streamlit）：
- 每个用户在自己的电脑上跑 `stats-code.exe`
- 后端绑 `127.0.0.1:8080`，不对外
- 数据落本地用户目录（`%APPDATA%\stats-code\`），无需多用户/认证/反代/集群

未来若要服务端多用户部署，把绑定地址换 `0.0.0.0` 加反代即可，但**不在当前范围内**。

## 分发与安装

**发布物**：单文件 `stats-code.exe`（Node SEA：内嵌 Node 运行时 + 前端 dist，
约 90 MB），随 zip 附 `install.ps1` + `SHA256SUMS.txt`。
构建入口：`scripts/release.ps1`（web build → ts-backend build → `npm run sea`
→ `npm run smoke` → 打包）。

**安装方式**：`install.ps1`：
- 复制 `stats-code.exe` 到 `%LOCALAPPDATA%\Programs\stats-code\`
- 把该目录加入用户级 PATH（不需要管理员权限）
- 创建桌面快捷方式

**进程形态**：纯前台进程，关闭窗口或 Ctrl+C 即停止。
不带系统托盘图标、不装 Windows 服务。用户机器**不需要 Node.js / R / Python**。

## 运行模式

### prod 模式（给最终用户）
- `scripts/release.ps1` 产出单文件 `stats-code.exe`（后端 + 内嵌 SPA）
- **桌面壳**：`desktop/` Electron 打包后产出 `Stats Code` 应用 / portable；
  启动时拉起 `stats-code.exe --no-browser`，在**应用内 WebView** 打开 UI
- 仍兼容：单独跑 `stats-code.exe` 用系统浏览器打开 `http://127.0.0.1:8080/`

### dev 模式（开发用，仅限自己）
- **桌面 GUI**：`scripts/start-desktop.ps1` 或 `cd desktop && npm start`
- **浏览器热更新**：仓库根 `启动Stats前端.bat`
  - `ts-backend/` `npm run build` → `node dev-server.mjs`（API `:8080`）+ Vite `:5173`
  - 浏览器访问 `http://localhost:5173/`，`/api/*` 代理到 `:8080`
- ⚠ `dev-server.mjs` 跑的是 `packages/api/dist/` **编译产物**——改完源码必须
  重新 build 才生效（bat 已内置该步骤）
- 必须本地装 Node.js ≥ 22 与 npm 依赖

## 启动行为

**端口选择**：从 `8080` 开始扫描，第一个能成功 `bind` 的端口即为本次实际端口。
启动日志打印实际端口；桌面壳通过轮询 `/api/health` 发现实际地址。

**UI 容器**：
- 桌面壳：应用内窗口（不调用系统浏览器）
- 裸 `stats-code.exe`：默认打开系统浏览器（`--no-browser` 跳过）

**单实例（Electron）**：桌面壳使用 Electron `requestSingleInstanceLock`；
二次启动聚焦已有窗口。后端 lock 文件实现见 `engine/launcher/lock`（逐步接线）。

## LLM 配置

**Key 存储**：JSON 文件，路径 `%APPDATA%\stats-code\llm-config.json`
（`packages/server/src/conversation/llm-config-store.ts`）。
原子写入（temp + rename）；文件损坏时自动改名备份为 `.corrupt-<时间戳>` 并视为未配置。
文件依赖 NTFS 默认权限（仅当前用户可读），不使用 DPAPI / Credential Manager。

**首次运行**：
- config 不存在或无 key，仍正常启动服务
- `/api/llm-status` 暴露 `{configured, provider, base_url, model}`
- 前端检测到未配置 → 主界面被居中卡片**遮罩**，不可聊天
- 卡片表单：provider 下拉 + API Key 输入 + 「测试并保存」按钮
- 点击保存 → 后端做一次 LLM 连通性测试 → 通过则写入 config 文件 → 卡片消失

**运行中异常**：
- 已配置但 LLM 调用失败 → 顶部横幅：「⚠ DeepSeek 连接异常 [重试] [修改 Key]」
- 「重试」= 重发上一条失败的用户消息
- 「修改 Key」= 弹回 Onboarding 卡片

## 角色

### 前端（frontend）
`web/`，React 19 + Vite + Ant Design。用户唯一面对的界面：双模式
（简易聊天 / 专业工作台），聊天、上传数据、选选项、看结果、等价代码侧栏、
审计快照导出。

### 后端（backend / server）
`ts-backend/packages/server`，HTTP 传输层 + 会话/消息编排 + LLM 接线。
13 条契约路由定义在 `src/contract/routes.ts`（zod schema，单一事实来源）。
会话持久化：`%APPDATA%\stats-code\sessions.json`（file-session-store）。

### 算法引擎（stats engine）
`ts-backend/packages/engine`，纯计算包：17 个 Output-Level 算法、数学核心
（distributions/linalg/special）、sidecar 渲染、snapshot/replay、redactor、
Spawn_Policy 哨兵（主动阻断对 R/SAS/Python/SPSS 等外部统计运行时的 spawn）。

### 应用组合（api）
`ts-backend/packages/api`，launcher 组装 + SEA 二进制入口。
依赖方向（eslint 强制）：**api → server → engine**。

## 信任凭证三件套

1. **等价代码侧栏**：每个分析附 R/SAS/Python/SPSS 四语言等价代码；
   模板在 `packages/engine/src/sidecar/templates/`，构建期嵌入。
2. **数值平价**：`tests/parity/` vitest 套件对照 32 个录制基线
   （`tests/parity/known_values/`），随 `npm test` 全量跑。
3. **审计快照**：导出确定性 zip（manifest/workflow/provenance/narrative），
   支持 `--replay` 完整性门控复现。

## 历史用法（已废弃）

- **整个 Rust workspace（4 crates，~65k LOC）** → 已退役删除；相关 git 分支/标签已清理
- Python `validation/` 平价套件 → 由 `ts-backend/tests/parity` 取代
- `cargo run -p stats-code -F dev-vite` dev 模式 → 由 `启动Stats前端.bat` 取代
- `%APPDATA%\stats-code\config.toml` → TS 后端读 `llm-config.json`，toml 为遗留文件
- `start.ps1` 双窗口启动脚本 → 已删除
- `stats-code chat` 终端 REPL / TUI → 已删除

## Spec 位置

- TS 重写：`.kiro/specs/typescript-backend-rewrite/`
- 会话编排：`.kiro/specs/ts-backend-conversation/`
- 双模式前端：`.kiro/specs/dual-mode-frontend/`
- 单命令启动器（历史规格）：`.kiro/specs/single-command-launcher/`
