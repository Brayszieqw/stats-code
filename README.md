# Stats Code

本地可审计的医学统计研究工作台（Windows）。

## 许可（请先读）

**专有软件 / All Rights Reserved。本公开仓库不提供完整源码。**

- 详见根目录 [`LICENSE`](./LICENSE)
- **未经版权所有者事先书面许可，不得使用、复制、修改、分发或商用**
- 浏览本仓库页面本身不构成授权；下载演示包也不自动获得使用许可
- 需要试用、评测或授权：请开 [Issues](https://github.com/Brayszieqw/stats-code/issues) 说明用途并等待书面同意

## 可运行演示包（需已获授权）

本仓库 **仅发布 Windows 演示包（Demo-Pack）**，不公开业务源码树。

在已获得书面许可的前提下，请从 Release 下载：

- [Releases · v0.1.0 Demo-Pack](https://github.com/Brayszieqw/stats-code/releases/tag/v0.1.0)

### 使用步骤（授权后）

1. 下载 `StatsCode-Demo-Pack-0.1.0-windows-x64.zip` 并解压
2. 双击 `start.bat`
3. 浏览器打开 `http://127.0.0.1:8080`
4. 选择专业模式 → 加载演示数据

### 演示包内含（概览）

| 内容 | 说明 |
|------|------|
| `stats-code.exe` | 可运行主程序（单文件分发） |
| `start.bat` / `install.bat` | 启动与安装辅助 |
| `data/demo_cohort.csv` | 演示数据集 |
| 说明文档 / SHA256SUMS | 使用说明与校验 |

解压后可运行包内 `verify-demo-pack.ps1` 做冷启动自检（可选）。

## 关于本仓库的 Git

本仓库 `main` 分支仅保留：

- 许可与产品说明（本 README + LICENSE）
- 演示相关说明（`docs/`）

**不包含** `web/`、`ts-backend/`、`desktop/` 等应用源码。完整源码仅在版权所有者本地保留，不随本公开仓库分发。

## 声明

本仓库默认 **不** 授予 OSI 开源许可证（MIT/Apache/GPL 等）。  
任何未在 `LICENSE` 与书面授权中写明的权利，均予保留。