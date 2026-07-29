# ADR-0001：以单文件 .exe + PowerShell 安装脚本分发，不做 MSI

- **状态**：Accepted
- **日期**：2026-05-19
- **相关 Spec**：`single-command-launcher`（历史规格，已归档至本地 work/backups/）

## 背景

`stats-code` 要从「项目根目录跑 `start.ps1`」演进为「PowerShell 任意位置敲 `stats-code` 即可拉起前后端」的一键启动产品。需求阶段考虑了几种 Windows 上的分发形态：

- **A. 单文件 `stats-code.exe` + `install.ps1` 安装脚本**：用户解压 zip → 右键 `install.ps1` → 装到 `%LOCALAPPDATA%\Programs\stats-code\` 并加用户级 PATH。
- **B. MSI / setup.exe 安装包**：用户双击向导，下一步下一步装到 `Program Files`。
- **C. Scoop / Chocolatey / winget 包**：开发者风格的命令行包管理。

用户最初倾向 B（向导式安装最贴近"无需复杂方法"），但走过权衡后选择 A。

## 决策

**采用方案 A**：单文件 `stats-code.exe` 通过 zip 分发，配套 `install.ps1` 完成安装。

具体形态：

- 发布物：`stats-code-<version>-windows-x64.zip`，内含 `stats-code.exe`、`install.ps1`、`README.md`、`SHA256SUMS.txt`。
- 安装路径：`%LOCALAPPDATA%\Programs\stats-code\`（无需管理员权限）。
- PATH：写入 `HKCU\Environment` 的用户级 PATH（无需 UAC）。
- 桌面快捷方式：`install.ps1` 创建一个指向 `stats-code.exe` 的快捷方式。
- 卸载：删除目录 + 从 PATH 移除（暂以 README 指引手动操作；未来可补 `uninstall.ps1`）。

## 备选方案及驳回理由

### 方案 B（MSI / setup.exe）

被驳回，主要负担：

1. **工具链**：必须维护 WiX / Inno Setup / NSIS 之一，每种都不是周末能搞定的事。
2. **代码签名**：未签名的 MSI 触发 SmartScreen「未知发布者」警告，目标用户（研究者、临床医生）容易劝退。EV 证书一年 $300+ 且必须用硬件 token，0.x 阶段不投入。
3. **管理员权限**：默认装 `Program Files` 要 UAC 提权。许多医院/研究所电脑给的是受限账户，直接卡住。
4. **升级语义**：`UpgradeCode` / `ProductCode` / 版本递增规则一旦写错会装出两份程序并存。
5. **不可逆动作**：MSI 写注册表、写"应用和功能"、必要时写服务，卸载时要全部清干净——出错会污染用户系统。
6. **CI 复杂度**：GitHub Actions 上调试 MSI 构建/签名问题远高于 zip。

A 形态的"复杂度"只剩一项：用户首次需右键 `install.ps1` → "用 PowerShell 运行"。距 MSI 双击向导差**一次右键操作**。

### 方案 C（Scoop / winget）

被驳回，因为目标用户画像是**研究者、临床医生、统计学习者**，不是开发者；他们普遍不会装 Scoop，也不熟悉 `winget install`。包管理器渠道留给未来产品成熟后补充，**不在当前范围**。

### 方案 D（portable zip，无 install.ps1）

被驳回，因为这意味着用户每次要敲完整路径或先 `cd`——回到要解决的原问题。

## 后果

### 正面

- **零外部依赖**：用户不需装 Node、不需装 Rust、不需装 Scoop；只要解压 + 运行一个脚本。
- **零权限要求**：所有操作在 `HKCU` 用户态完成，避开 UAC 和管理员限制。
- **升级简单**：替换 `stats-code.exe` 即可，无版本号规则、无注册表清理。
- **卸载简单**：删目录 + 从 PATH 移除，可逆。
- **CI 简单**：`cargo build --release` + `Compress-Archive` 即可。
- **决策可逆**：未来产品成熟、有签名预算后升级到 MSI / winget，不破坏现有用户体验（A 是 B 的真子集）。

### 负面

- **首次安装需要一次"用 PowerShell 运行"**，比 MSI 双击向导多一步操作。
- **PowerShell 执行策略**：默认策略可能阻止 `install.ps1`，需在 README 指引执行 `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned` 或 `powershell -ExecutionPolicy Bypass -File install.ps1`。
- **没有"应用和功能"卸载入口**：用户卸载需手动操作或运行 `uninstall.ps1`。
- **SmartScreen 仍可能弹出**（首次运行未签名 .exe 时），但 zip 形态触发率低于 MSI/setup.exe。

## 触发重新评估的条件

满足任意一条即重新审视本决策：

- 用户群从研究者扩展到普通消费者，对"应用和功能"卸载入口有刚性需求。
- 拿到代码签名预算，可解决 SmartScreen 警告。
- 出现明确的企业部署需求（IT 部门要求 MSI 包以走内部分发渠道）。
- `install.ps1` 反馈频繁失败（PowerShell 策略、防病毒拦截等）。
