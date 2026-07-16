<#
.SYNOPSIS
  Installs stats-code.exe to %LOCALAPPDATA%\Programs\stats-code\.

.DESCRIPTION
  User-level installer (no UAC required):
    1. Copy   - copy stats-code.exe (+ optional demo data / start.bat) to Install_Dir.
    2. PATH   - append Install_Dir to HKCU\Environment\Path (idempotent).
    3. Shortcut - create/overwrite "Stats Code.lnk" on the desktop.

  Security:
    - Never copies API keys, llm-config.json, .env, or any credential files.
    - LLM keys must be configured by each user inside the app after install.

  This script is part of the Distribution_Archive / Demo-Pack and is expected
  to live next to stats-code.exe at install time.
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ---------------------------------------------------------------------------
# Step 1 - Copy stats-code.exe to %LOCALAPPDATA%\Programs\stats-code\
# ---------------------------------------------------------------------------

$Install_Dir = Join-Path $env:LOCALAPPDATA 'Programs\stats-code'
$Source_Exe  = Join-Path $PSScriptRoot 'stats-code.exe'
$Target_Exe  = Join-Path $Install_Dir 'stats-code.exe'

if (-not (Test-Path -LiteralPath $Source_Exe -PathType Leaf)) {
    Write-Error "未找到 stats-code.exe（期望路径：$Source_Exe）。请确认 install.ps1 与 stats-code.exe 位于同一目录后再运行。"
    exit 1
}

# Refuse to package-install anything that looks like a secrets file in the source dir.
$forbiddenNames = @(
    'llm-config.json',
    '.env',
    '.env.local',
    'env.json',
    'secrets.json',
    'credentials.json'
)
foreach ($name in $forbiddenNames) {
    $hit = Join-Path $PSScriptRoot $name
    if (Test-Path -LiteralPath $hit -PathType Leaf) {
        Write-Error "检测到不应分发的密钥文件：$name。请从安装包目录删除后再安装（密钥只应保存在本机 %APPDATA%\stats-code\）。"
        exit 1
    }
}

if (-not (Test-Path -LiteralPath $Install_Dir -PathType Container)) {
    try {
        New-Item -ItemType Directory -Path $Install_Dir -Force | Out-Null
    } catch {
        Write-Error "无法创建安装目录 $Install_Dir：$($_.Exception.Message)"
        exit 1
    }
}

try {
    Copy-Item -LiteralPath $Source_Exe -Destination $Target_Exe -Force -ErrorAction Stop
} catch [System.IO.IOException] {
    Write-Error @"
拷贝 stats-code.exe 失败：目标文件被占用。
请先关闭运行中的 stats-code 实例（关闭 PowerShell 中的 stats-code 命令窗口或在任务管理器中结束 stats-code.exe 进程），然后重新运行 install.ps1。
目标路径：$Target_Exe
原始错误：$($_.Exception.Message)
"@
    exit 1
} catch {
    Write-Error "拷贝 stats-code.exe 到 $Target_Exe 失败：$($_.Exception.Message)"
    exit 1
}

Write-Host "Copy complete: $Target_Exe"

# Optional demo dataset (never required; no secrets)
$Source_Data_Dir = Join-Path $PSScriptRoot 'data'
$Source_Demo     = Join-Path $Source_Data_Dir 'demo_cohort.csv'
if (Test-Path -LiteralPath $Source_Demo -PathType Leaf) {
    $Target_Data_Dir = Join-Path $Install_Dir 'data'
    New-Item -ItemType Directory -Path $Target_Data_Dir -Force | Out-Null
    Copy-Item -LiteralPath $Source_Demo -Destination (Join-Path $Target_Data_Dir 'demo_cohort.csv') -Force
    Write-Host "Demo data copied: $Target_Data_Dir\demo_cohort.csv"
}

# Portable start helper next to the installed exe
$Start_Bat = Join-Path $Install_Dir 'start.bat'
$startBatContent = @"
@echo off
setlocal EnableExtensions
cd /d "%~dp0"
if not exist "%~dp0stats-code.exe" (
  echo [ERROR] stats-code.exe not found.
  pause
  exit /b 1
)
start "" "%~dp0stats-code.exe"
exit /b 0
"@
[System.IO.File]::WriteAllText($Start_Bat, $startBatContent, [System.Text.UTF8Encoding]::new($false))
Write-Host "Start helper written: $Start_Bat"

# ---------------------------------------------------------------------------
# Step 2 - HKCU\Environment\Path 幂等更新（user-level，无需 UAC）
# ---------------------------------------------------------------------------

$Current_Path = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not $Current_Path) { $Current_Path = '' }

$Existing_Entries = $Current_Path -split ';' |
    ForEach-Object { $_.Trim() } |
    Where-Object { $_ -ne '' }

$Normalized_Entries = @($Existing_Entries | ForEach-Object {
    ($_ -replace '\\+$', '').ToLowerInvariant()
})
$Install_Dir_Norm = ($Install_Dir -replace '\\+$', '').ToLowerInvariant()

if ($Normalized_Entries -contains $Install_Dir_Norm) {
    Write-Host "PATH already contains Install_Dir, skipping append: $Install_Dir"
} else {
    if ([string]::IsNullOrEmpty($Current_Path)) {
        $New_Path = $Install_Dir
    } elseif ($Current_Path.EndsWith(';')) {
        $New_Path = "$Current_Path$Install_Dir"
    } else {
        $New_Path = "$Current_Path;$Install_Dir"
    }

    [Environment]::SetEnvironmentVariable('Path', $New_Path, 'User')
    Write-Host "PATH updated: appended $Install_Dir"
}

# ---------------------------------------------------------------------------
# Step 3 - 桌面快捷方式（覆盖式创建）
# ---------------------------------------------------------------------------

$Desktop_Dir    = [Environment]::GetFolderPath('Desktop')
$Shortcut_Path  = Join-Path $Desktop_Dir 'Stats Code.lnk'
$WshShell = $null
$Shortcut = $null

try {
    $WshShell = New-Object -ComObject WScript.Shell
    $Shortcut = $WshShell.CreateShortcut($Shortcut_Path)
    $Shortcut.TargetPath       = $Target_Exe
    $Shortcut.WorkingDirectory = $Install_Dir
    $Shortcut.Description      = 'Stats Code 统计分析智能体（无需 API Key 即可用专业模式）'
    $Shortcut.Save()
    Write-Host "Desktop shortcut created: $Shortcut_Path"
} catch {
    Write-Error "创建桌面快捷方式失败：$($_.Exception.Message)"
    exit 1
} finally {
    if ($null -ne $Shortcut) { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($Shortcut) }
    if ($null -ne $WshShell) { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($WshShell) }
}

Write-Host ''
Write-Host 'Install complete.'
Write-Host "  exe     : $Target_Exe"
Write-Host "  start   : $Start_Bat"
Write-Host '  note    : No API keys were installed. Configure LLM later inside the app if needed.'
Write-Host '  tip     : Professional mode works without any API key.'

exit 0
