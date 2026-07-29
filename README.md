# Stats Code

本地可审计的医学统计研究工作台（Windows）。

## 许可（请先读）

**专有软件 / All Rights Reserved。源码可见 ≠ 开源。**

- 详见根目录 [`LICENSE`](./LICENSE)
- **未经版权所有者事先书面许可，不得使用、复制、修改、分发或商用**
- 浏览本仓库页面本身不构成授权；克隆/下载也不自动获得使用权
- 需要试用、评测或授权：请开 [Issues](https://github.com/Brayszieqw/stats-code/issues) 说明用途并等待书面同意

## 可运行演示包（需已获授权）

在已获得书面许可的前提下，可从 Release 获取 Windows 演示包：

- [Releases · v0.1.0 Demo-Pack](https://github.com/Brayszieqw/stats-code/releases/tag/v0.1.0)

典型步骤（授权后）：

1. 下载 `StatsCode-Demo-Pack-0.1.0-windows-x64.zip` 并解压  
2. 双击 `start.bat`  
3. 浏览器打开 `http://127.0.0.1:8080`  
4. 选择专业模式 → 加载演示数据  

## 源码结构（概览）

| 目录 | 说明 |
|------|------|
| `ts-backend/` | Node.js 后端（API / server / 统计引擎） |
| `web/` | 前端 SPA |
| `desktop/` | Electron 桌面壳 |
| `docs/` / `knowledge-base/` | 文档与领域说明 |
| `packaging/` | 分发与同事演示说明 |

开发与领域约定见 [`CONTEXT.md`](./CONTEXT.md)。

## 声明

本仓库默认**不**授予 OSI 开源许可证（MIT/Apache/GPL 等）。  
任何未在 `LICENSE` 与书面授权中写明的权利，均予保留。
