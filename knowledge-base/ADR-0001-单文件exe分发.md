---
type: adr
status: accepted
tags: [adr]
---

# ADR-0001:单文件 exe 分发(非 MSI)

> 发布物为单文件 `stats-code.exe`(~30~80 MB,含内嵌前端 dist),通过 GitHub Releases zip + `install.ps1` 分发。不用 MSI/代码签名/Scoop/winget(0.x 阶段优先简单可逆)。

## 决策
- 安装:`install.ps1` 复制 exe 到 `%LOCALAPPDATA%\Programs\stats-code\`,加用户级 PATH,建桌面快捷方式,免管理员权限。
- 进程:纯前台 PowerShell,关窗即停。无托盘、无 Windows 服务。

## 影响
- 决定了 [[单命令启动器]] 的形态。
- TS 版用 Node SEA 实现单文件(见 [[TS-api]] / [[ADR-0003-TS后端进程内算法]])。

## 相关
- [[单命令启动器]] · [[领域语言]]
