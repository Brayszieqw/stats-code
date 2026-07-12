# Stats Code Desktop（应用内窗口）

把现有本地后端 + Web 前端放进 **Electron 窗口**里运行，交互形态接近 Codex：

- 双击 / `npm start` → **应用内界面**
- 后端以 `--no-browser` 启动，**不再打开系统浏览器**
- 关闭窗口即停止本壳启动的后端

## 依赖

- Node.js ≥ 22（仅开发与打包机需要；最终用户跑 portable 包不需要）
- 已构建的后端：`ts-backend/build/stats-code.exe`（优先）  
  或 `ts-backend/packages/api/dist/bin.js`（开发回退）

## 开发启动

```powershell
# 仓库根
powershell -ExecutionPolicy Bypass -File .\scripts\start-desktop.ps1

# 或
cd desktop
npm install
npm start
```

可选：

```powershell
$env:STATS_CODE_BACKEND = "D:\path\to\stats-code.exe"
npm start
```

## 无界面冒烟

```powershell
cd desktop
npm run smoke
```

## 打包便携版（Windows）

先保证 `ts-backend/build/stats-code.exe` 存在（`scripts/release.ps1` 或 `npm run sea`），然后：

```powershell
cd desktop
npm run dist
```

产物在 `desktop/dist/`（portable exe + unpacked dir）。`extraResources` 会带上 `backend/stats-code.exe`。

## 与「浏览器模式」的关系

| 入口 | UI 容器 |
|------|---------|
| `desktop` / `start-desktop.ps1` | **应用内 WebView（推荐）** |
| `stats-code.exe`（默认） | 仍可打开系统浏览器（兼容旧演示脚本） |
| `stats-code.exe --no-browser` | 仅后端，供桌面壳或其他宿主连接 |

## 安全边界

- 页面 `contextIsolation + sandbox`，无 Node 集成
- 仅允许导航到 `127.0.0.1` / `localhost`；外链用系统浏览器打开
- 统计数值仍只走本机确定性引擎（与 Web 版相同）
