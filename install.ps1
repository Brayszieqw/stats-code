<#
.SYNOPSIS
  Installs stats-code.exe to %LOCALAPPDATA%\Programs\stats-code\.

.DESCRIPTION
  Three-step user-level installer (no UAC required):
    1. Copy   - copy stats-code.exe to Install_Dir.
    2. PATH   - append Install_Dir to HKCU\Environment\Path (idempotent).  [task 13.2]
    3. Shortcut - create/overwrite "Stats Code.lnk" on the desktop.        [task 13.4]

  This script is part of the Distribution_Archive (single-command-launcher spec)
  and is expected to live next to stats-code.exe at install time.

.NOTES
  Requirements covered by this revision: 14.1, 14.2, 14.3, 14.6
  Desktop shortcut (14.4, 14.5) comes in task 13.4.
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
    # 非 IOException 的失败（例如权限不足）也按非零退出，避免静默成功。
    Write-Error "拷贝 stats-code.exe 到 $Target_Exe 失败：$($_.Exception.Message)"
    exit 1
}

Write-Host "Copy complete: $Target_Exe"

# ---------------------------------------------------------------------------
# Step 2 - HKCU\Environment\Path 幂等更新（user-level，无需 UAC）
# ---------------------------------------------------------------------------
#
# 关键点：
#   * 用 [Environment]::GetEnvironmentVariable('Path','User') 读取，等价于
#     `Get-ItemProperty 'HKCU:\Environment' Path` 但缺失时返回 $null 而不会抛错。
#   * 写回用 [Environment]::SetEnvironmentVariable(..., 'User')，它在写入
#     HKCU\Environment\Path 的同时广播 WM_SETTINGCHANGE，让已运行的 Shell 也
#     刷新 PATH。直接 Set-ItemProperty 不会广播，需要手动 P/Invoke，故选前者。
#   * 比较时按 `;` split、去空、去末尾反斜杠、忽略大小写，避免重复追加。

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
#
# 关键点：
#   * 用 WScript.Shell COM CreateShortcut() 创建 .lnk 文件。
#   * 每次运行直接覆盖同名快捷方式（CreateShortcut 打开已存在的 .lnk 时等同编辑）
#     → 不会产生 `Stats Code (2).lnk` 副本。
#   * 桌面路径通过 [Environment]::GetFolderPath('Desktop') 获取，
#     比 `$env:USERPROFILE\Desktop` 更健壮（支持重定向桌面到 OneDrive 等场景）。

$Desktop_Dir    = [Environment]::GetFolderPath('Desktop')
$Shortcut_Path  = Join-Path $Desktop_Dir 'Stats Code.lnk'

try {
    $WshShell = New-Object -ComObject WScript.Shell
    $Shortcut = $WshShell.CreateShortcut($Shortcut_Path)
    $Shortcut.TargetPath       = $Target_Exe
    $Shortcut.WorkingDirectory = $Install_Dir
    $Shortcut.Description      = 'Stats Code 统计分析智能体'
    $Shortcut.Save()
    Write-Host "Desktop shortcut created: $Shortcut_Path"
} catch {
    Write-Error "创建桌面快捷方式失败：$($_.Exception.Message)"
    exit 1
} finally {
    # 释放 COM 对象
    if ($null -ne $Shortcut) { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($Shortcut) }
    if ($null -ne $WshShell) { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($WshShell) }
}

exit 0
