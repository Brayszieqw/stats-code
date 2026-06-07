# Stats Code · 领域语言

> 共享词汇表。当代码里出现这些词，按这里的定义理解；当用户口头使用这些词，按这里的定义对齐。

## 产品命令

### `stats-code`（命令）
全局可执行的二进制（`stats-code.exe`，放在 PATH 里）。

**对用户暴露的唯一行为**：在任意目录敲 `stats-code`（不带子命令）会：
1. 启动后端 HTTP 服务（axum，监听 `:8080`）
2. 启动前端（dev：vite `:5173`；prod：从后端直接吐内嵌的静态资源）
3. 自动打开浏览器到前端地址
4. 前台运行，按 Ctrl+C 停止全部

唯一的命令行旗标：`--version`、`--help`。**不再有公开子命令。**

### `stats-code` 二进制内部的算法子命令
`tableone`、`survival`、`ttest`、`power`、`workflow`、`init`、`doctor`、`chat` 等——
**不是公开 CLI 用法**，而是 `agent-core` 通过 `SkillInvoker::StatsCli` 子进程调用的内部 API。
代码继续保留，但用户视角不可见、`--help` 不列出。

## 部署形态

**本地单机模式**（localhost 应用，类似 OpenCode / Jupyter Lab / Streamlit）：
- 每个用户在自己的电脑上跑 `stats-code.exe`
- 后端绑 `127.0.0.1:8080`，不对外
- 前端通过 `http://localhost:5173`（dev）或 `http://localhost:8080`（prod）访问
- 数据落本地用户目录，无需多用户/认证/反代/集群

未来若要服务端多用户部署，把绑定地址换 `0.0.0.0` 加反代即可，但**不在当前范围内**。

## 分发与安装

**发布物**：单文件 `stats-code.exe`（约 30~80 MB，含内嵌前端 dist），通过 GitHub Releases 提供 zip 下载。

**安装方式**：随 zip 附带的 `install.ps1`：
- 复制 `stats-code.exe` 到 `%LOCALAPPDATA%\Programs\stats-code\`
- 把该目录加入用户级 PATH（不需要管理员权限）
- 创建桌面快捷方式
- 用户操作只有一步：右键 `install.ps1` → "用 PowerShell 运行"

**进程形态**：纯前台 PowerShell 进程，关闭窗口或 Ctrl+C 即停止。
不带系统托盘图标、不装 Windows 服务。

**未来可能升级**：MSI / 代码签名 / Scoop / winget——但**不在当前范围内**，
当前 0.x 阶段优先简单可逆。

## 运行模式

`stats-code` 二进制有两种运行模式，由 Cargo feature `dev-vite` 切换：

### prod 模式（默认，给最终用户）
- `cargo build --release` 产出
- 前端 `web/dist/` 在编译期通过 `rust-embed` / `include_dir!` 嵌入二进制
- 单进程：axum 在 `:8080` 同时伺服 API（`/api/*`）和静态资源（`/*`）
- 浏览器访问 `http://localhost:8080/`
- 用户机器**不需要 Node.js**

### dev 模式（开发用，仅限自己）
- `cargo run -p stats-code -F dev-vite`
- Rust 主进程 spawn 一个 vite dev server 子进程（`npm run dev`）
- 同时启 axum :8080 + vite :5173
- 浏览器访问 `http://localhost:5173/`，vite 把 `/api/*` 代理到 :8080
- Rust 主进程退出（Ctrl+C）时**保证**杀掉 vite 子进程，无残留
- 必须本地装 Node.js 与 npm 依赖

## 启动行为

**端口选择**：从 `8080` 开始扫描，第一个能成功 `bind` 的端口即为本次实际端口。
扫描范围 `8080..8200`，全部占用则报错退出。
启动日志打印实际端口，浏览器自动打开的 URL 也跟随实际端口。

**浏览器**：`stats-code` 起来后自动用系统默认浏览器打开实际地址。

**单实例**：检测已有实例运行 → 直接打开浏览器到那个实例的 URL，不再起新进程。
实现：在 `%APPDATA%\stats-code\running.lock` 写当前实例的 PID + URL；
启动时若文件存在、PID 仍活、端口可访问，则 `start <url>` 后退出。
进程退出时清理 lock 文件。

## LLM 配置

**Key 存储**：明文 TOML 文件，路径 `%APPDATA%\stats-code\config.toml`。
文件依赖 NTFS 默认权限（仅当前用户可读）。
不使用 DPAPI、不使用 Windows Credential Manager。

**首次运行**：
- 启动时若 config 文件不存在或无 key，仍正常启动服务
- `/api/health`（或专属端点）暴露 `{configured: bool, provider: string|null}`
- 前端检测到未配置 → 主界面被居中卡片**遮罩**，不可聊天
- 卡片表单：provider 下拉 + API Key 输入 + 「测试并保存」按钮
- 点击保存 → 后端做一次 LLM 连通性测试 → 通过则写入 config 文件 → 卡片消失

**运行中异常**：
- 已配置但 LLM 调用失败 → 顶部横幅：「⚠ DeepSeek 连接异常 [重试] [修改 Key]」
- 「重试」= 重发上一条失败的用户消息
- 「修改 Key」= 弹回 Onboarding 卡片

## 角色

### 前端（frontend）
`web/`，Vite + TypeScript。用户唯一面对的界面：聊天、上传文件、选选项、看结果。

### 后端（backend / agent-server）
`crates/agent-server`，axum HTTP 服务。接前端请求、调 LLM、调用算法。
本身不实现任何统计算法。

### 算法引擎（stats engine）
`crates/stats-code` 内部的 `stats/`、`tableone.rs`、`survival.rs` 等模块。
被 `agent-core` 通过 spawn `stats-code <subcommand> --json` 子进程调用。

> ⚠ **TS 后端不同**（见 ADR-0003）：TypeScript 重写（`ts-backend/`）把算法实现为
> **进程内纯函数**（`packages/engine/src/stats/*.ts`），由 HTTP 层直接调用，**不 spawn 子进程**，
> 并用 Spawn_Policy 哨兵主动阻断对外部统计运行时（R/SAS/Python/SPSS）的 spawn。
> 本节描述的子进程模型仅适用于 Rust 版。

### Agent Core
`crates/agent-core`，业务编排层。在后端进程内运行，
负责会话、消息流、Skill 注册表、把统计请求路由到 stats engine。

> ⚠ **TS 后端不同**（见 ADR-0004）：TypeScript 重写采用三层包结构
> `api → server → engine`，**没有独立的 agent-core 对应包**。会话/消息编排/LLM
> 等职责落在 `packages/server` 内（`state.ts`、`mem-store.ts`、`llm.ts`、`sse.ts`）。

## TS 后端架构速览（见 `.kiro/specs/typescript-backend-rewrite/` + ADR-0003/0004/0005）

- **包结构**：`api`（应用组合 + SEA bin 入口）→ `server`（HTTP 传输 + 编排）→ `engine`（纯计算）。
- **算法**：17 个 Output-Level 算法为进程内纯函数，禁止 spawn 外部统计运行时（ADR-0003）。
- **产物**：Node SEA 单文件 `stats-code.exe`，内嵌运行时 + 前端资源，零外部依赖。
- **Power 家族**：刻意对齐 SAS PROC POWER 而非 Rust normal-approximation（ADR-0005）——
  这是唯一刻意偏离「TS↔Rust 数值等价」的算法。

## 历史用法（已废弃）

- `start.ps1` 双窗口启动脚本 → **已删除**，由 `stats-code` 单命令取代
- `package-release.ps1` 旧版打包（只装 CLI 不带前端）→ **已删除**，由新的 `release.ps1` 重写
- 公开的 `stats-code init / doctor / workflow run` 用法 → 用户不再直接敲，但代码保留供 `SkillInvoker::StatsCli` 内部使用
- `stats-code chat` 终端 REPL → **已删除**（v0.x），chat 模块与 ui/ TUI 子系统一并移除；`SkillInvoker::StatsCli` 注册表不调用 chat 子命令

## Spec 位置

本特性的 spec：`.kiro/specs/single-command-launcher/`
