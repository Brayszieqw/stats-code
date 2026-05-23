//! Web_Frontend 伺服 / 启动入口（Requirements 6, 7）。
//!
//! 真正的实现分两条路径：
//! - **prod 模式**（默认 feature）通过 rust-embed 把 `web/dist/` 嵌入二进制，
//!   由 Agent_Backend 直接伺服；本模块的 [`ensure_frontend`] 仅返回
//!   [`FrontendHandle::EmbeddedProd`]，不需要管理外部进程，公开 URL 即调用
//!   方传入的 `backend_url`（Requirement 6.2, 6.3）。
//! - **dev 模式**（feature `dev-vite`）spawn `npm run dev` 子进程，并通过
//!   [`crate::launcher::process_guard::ProcessGuard`] 把它绑定到主进程生命
//!   周期上（Requirement 7.1, 7.3）。本函数只负责「拉起 + 等就绪」；
//!   主进程仍存活时 Vite 异常退出（[`FrontendError::ViteExitedEarly`]）的
//!   监听由 [`crate::launcher::Launcher::run`]（task 8.1）派生独立线程承担，
//!   见 task 15.2。本函数返回 `Ok` 时调用方应自行 spawn 一个监视 task。
//!
//! `FrontendError` 的变体与 design.md「Error Handling」表对齐：
//! - [`FrontendError::ViteSpawnFailed`] → exit 4
//! - [`FrontendError::ViteExitedEarly`] → exit 5
//! - [`FrontendError::EmbeddedAssetMissing`] → prod 模式下 rust-embed 找不到
//!   `index.html` 时使用，编译期 build.rs 已经先做防御。

use thiserror::Error;

use crate::launcher::process_guard::ProcessGuard;

/// [`ensure_frontend`] 的返回值；指示前端来源。
#[derive(Debug)]
pub enum FrontendHandle {
    /// prod 模式：什么都不需要管理，公开 URL 即调用方传入的 backend URL。
    EmbeddedProd,
    /// dev 模式：携带 Vite 子进程守护句柄与该子进程对外暴露的 URL。
    DevVite(ProcessGuard, String),
}

/// 前端启动相关错误。
///
/// 变体集合覆盖 design.md「Error Handling」表中所有与前端相关的诊断条目，
/// 即便当前任务只接线了 prod 分支：保留全部变体可以让 dev 分支在 task 7.2
/// 接入时无需再次调整公共 API。
#[derive(Debug, Error)]
pub enum FrontendError {
    /// dev 模式下 `npm run dev` 启动失败（launcher exit code 4）。
    #[error("Vite 子进程启动失败：{0}")]
    ViteSpawnFailed(String),

    /// dev 模式下 Vite 在主进程仍存活时异常退出（launcher exit code 5）。
    #[error("Vite 子进程在主进程仍存活时异常退出")]
    ViteExitedEarly,

    /// prod 模式下内嵌资源中缺少 `index.html`；编译期 build.rs 已经先做
    /// 防御，运行时若仍触发说明发布物被破坏，调用方应 panic。
    #[error("内嵌前端资源中缺少 index.html")]
    EmbeddedAssetMissing,
}

/// 确保前端可达；返回 [`FrontendHandle`]。
///
/// prod 分支不依赖 `backend_url`（前端由 Agent_Backend 在同一端口下伺服），
/// 但参数保留以与 dev 分支签名对齐；调用方在 prod 分支下应直接以
/// `backend_url` 作为对外公开的 URL。
///
/// # Errors
/// prod 分支当前不会返回错误；返回类型保留 [`FrontendError`] 是为了与 dev
/// 分支统一签名，并允许后续 task 在不破坏 ABI 的前提下增加 prod 模式诊断
/// （例如 [`FrontendError::EmbeddedAssetMissing`]）。
#[cfg(not(feature = "dev-vite"))]
pub fn ensure_frontend(_backend_url: &str) -> Result<FrontendHandle, FrontendError> {
    Ok(FrontendHandle::EmbeddedProd)
}

// ---------------------------------------------------------------------------
// dev 分支实现（feature = "dev-vite"）
// ---------------------------------------------------------------------------

/// dev 模式下 Vite_Dev_Server 的对外 URL。固定为 `127.0.0.1:5173`，与
/// design.md「Lock_File 格式」中的 dev 模式 URL 与 Requirement 3.3
/// 「dev 模式 Vite 仅监听 127.0.0.1」对齐。
#[cfg(feature = "dev-vite")]
const VITE_DEV_URL: &str = "http://127.0.0.1:5173/";

/// 等待 Vite 输出就绪标记的硬上限。Vite 5 在中型项目首次启动通常 1~3 秒，
/// 10s 兜底既能容忍冷启动也能在死锁 / npm install 缺失时尽快报错。
#[cfg(feature = "dev-vite")]
const VITE_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// dev 分支：在 `<workspace>/web/` 目录下 spawn `npm run dev`，等待其输出
/// 就绪标记后返回受守护的句柄。
///
/// 步骤（与 task 7.2 描述一一对应）：
/// 1. 解析 web 目录路径：`CARGO_MANIFEST_DIR / ../../web`，避免硬编码绝对
///    路径，便于在 CI / 多用户环境复用。
/// 2. 构造 `cmd /c npm run dev -- --host 127.0.0.1`，让 stdout/stderr 走
///    管道以便扫描 ready 行（Requirement 3.3 / 7.1）。
///    > 注：仓库内 `web/vite.config.ts` 未显式设 `server.host`，本函数通过
///    > 命令行旗标确保 Vite 仅监听 `127.0.0.1`，与 Requirement 3.3 对齐。
/// 3. 通过 [`ProcessGuard::spawn_in_job`] 把 Vite 绑定到 Job Object，主
///    进程退出即同步杀子进程（Requirement 7.3 由 ProcessGuard 提供）。
/// 4. 起一个后台线程持续读 stdout，扫描 `Local:` / `ready in` 任一就绪
///    标记。在 [`VITE_READY_TIMEOUT`] 内未观察到 → kill 守护并返回
///    [`FrontendError::ViteSpawnFailed`]（Requirement 7.4）。
/// 5. 早退监听（child.wait()，对应 [`FrontendError::ViteExitedEarly`]）
///    交由 `Launcher::run` task 8.1 / task 15.2 派生独立线程完成；本函数
///    只在 ready 阶段把「stdout pipe 在就绪前关闭」视为早退诊断。
///
/// # Errors
/// - [`FrontendError::ViteSpawnFailed`]：spawn 失败、stdout 不可用、就绪
///   超时、或在就绪前 Vite 进程已终止（pipe 关闭）。
#[cfg(feature = "dev-vite")]
pub fn ensure_frontend(backend_url: &str) -> Result<FrontendHandle, FrontendError> {
    let web_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("web");
    ensure_frontend_in_dir(&web_dir, backend_url)
}

/// [`ensure_frontend`] 的内部实现，把 web 目录暴露成参数以便单元测试在不
/// 真正 spawn Vite 的前提下覆盖错误路径（传入不存在的目录使 spawn 失败）。
#[cfg(feature = "dev-vite")]
fn ensure_frontend_in_dir(
    web_dir: &std::path::Path,
    _backend_url: &str,
) -> Result<FrontendHandle, FrontendError> {
    use std::io::{BufRead, BufReader};
    use std::sync::mpsc;
    use std::thread;

    let cmd = build_npm_dev_command(web_dir);

    let mut guard = ProcessGuard::spawn_in_job(cmd).map_err(|e| {
        FrontendError::ViteSpawnFailed(format!("spawn `npm run dev` failed: {e}"))
    })?;

    // spawn 已成功；后续任何错误路径都需要 kill 守护避免泄漏 Vite 进程。
    let stdout = match guard.child_stdout_take() {
        Some(s) => s,
        None => {
            let _ = guard.kill();
            return Err(FrontendError::ViteSpawnFailed(
                "Vite 子进程 stdout 管道不可用".to_string(),
            ));
        }
    };
    let stderr = guard.child_stderr_take();

    // ready 信号通过 mpsc 单次传递；ready 后线程继续读 stdout 以排空管道，
    // 防止 Vite 写满管道阻塞自己。
    let (tx, rx) = mpsc::channel::<()>();
    thread::Builder::new()
        .name("vite-stdout-scanner".into())
        .spawn(move || {
            let reader = BufReader::new(stdout);
            let mut signaled = false;
            for line in reader.lines() {
                let Ok(l) = line else { break };
                // 转发到主进程 stdout，便于用户在 PowerShell 中观察启动进度。
                println!("[vite] {l}");
                if !signaled && (l.contains("Local:") || l.contains("ready in")) {
                    let _ = tx.send(());
                    signaled = true;
                }
            }
            // 离开循环 → pipe 关闭 / 解析错误；tx drop 让 rx 看到 Disconnected。
        })
        .map_err(|e| {
            FrontendError::ViteSpawnFailed(format!("failed to spawn vite scanner thread: {e}"))
        })?;

    if let Some(stderr) = stderr {
        let _ = thread::Builder::new()
            .name("vite-stderr-drain".into())
            .spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    if let Ok(l) = line {
                        eprintln!("[vite] {l}");
                    } else {
                        break;
                    }
                }
            });
    }

    match rx.recv_timeout(VITE_READY_TIMEOUT) {
        Ok(()) => Ok(FrontendHandle::DevVite(guard, VITE_DEV_URL.to_string())),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let _ = guard.kill();
            Err(FrontendError::ViteSpawnFailed(format!(
                "Vite 启动超时（{} 秒内未观察到 ready 标记）",
                VITE_READY_TIMEOUT.as_secs()
            )))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            // 扫描线程结束 → stdout 已关闭 → Vite 在就绪前已退出。
            let _ = guard.kill();
            Err(FrontendError::ViteSpawnFailed(
                "Vite 子进程在输出 ready 标记前已退出".to_string(),
            ))
        }
    }
}

/// Windows: 通过 `cmd /c` 调起 `npm.cmd`（npm 实质是批处理）。
/// 强制 `--host 127.0.0.1` 满足 Requirement 3.3。
#[cfg(all(feature = "dev-vite", windows))]
fn build_npm_dev_command(web_dir: &std::path::Path) -> std::process::Command {
    use std::process::Stdio;
    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/c", "npm", "run", "dev", "--", "--host", "127.0.0.1"])
        .current_dir(web_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

/// 非 Windows 兜底：保持源码可在 Linux CI 上 `cargo check --features dev-vite`
/// 通过；运行时 [`ProcessGuard::spawn_in_job`] 会立刻返回 UnsupportedPlatform。
#[cfg(all(feature = "dev-vite", not(windows)))]
fn build_npm_dev_command(web_dir: &std::path::Path) -> std::process::Command {
    use std::process::Stdio;
    let mut cmd = std::process::Command::new("npm");
    cmd.args(["run", "dev", "--", "--host", "127.0.0.1"])
        .current_dir(web_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    /// prod 分支：调用方拿到的句柄就是 `EmbeddedProd`。
    #[cfg(not(feature = "dev-vite"))]
    #[test]
    fn prod_returns_embedded_prod() {
        let handle = ensure_frontend("http://127.0.0.1:8080/").expect("prod 不应失败");
        assert!(
            matches!(handle, FrontendHandle::EmbeddedProd),
            "prod 模式应返回 EmbeddedProd，实际：{handle:?}"
        );
    }

    /// dev 分支冒烟测试：把 `web` 目录指向一个不存在的路径，让底层
    /// `Command::spawn`（Windows 上 `CreateProcess` 因 `lpCurrentDirectory`
    /// 不存在）或 `ProcessGuard::spawn_in_job`（非 Windows 平台 stub 直接返回
    /// `UnsupportedPlatform`）失败，从而走到 [`FrontendError::ViteSpawnFailed`]
    /// 路径。这条测试故意 **不** 真的 spawn `npm`，避免对开发机 Node 环境的
    /// 依赖与 ~10 秒的等待。
    #[cfg(feature = "dev-vite")]
    #[test]
    fn dev_returns_vite_spawn_failed_when_web_dir_missing() {
        let bogus = std::path::PathBuf::from(
            "Z:\\__definitely_no_such_dir_for_stats_code_dev_vite_test__",
        );
        let err = ensure_frontend_in_dir(&bogus, "http://127.0.0.1:8080/")
            .expect_err("missing web dir 必须导致 ViteSpawnFailed");
        assert!(
            matches!(err, FrontendError::ViteSpawnFailed(_)),
            "应返回 ViteSpawnFailed，实际：{err:?}"
        );
    }

    /// `FrontendError` 三变体的 `Display` 输出非空，便于诊断日志。
    #[test]
    fn error_variants_have_nonempty_display() {
        let cases = [
            FrontendError::ViteSpawnFailed("npm not found".into()),
            FrontendError::ViteExitedEarly,
            FrontendError::EmbeddedAssetMissing,
        ];
        for err in cases {
            let rendered = err.to_string();
            assert!(!rendered.is_empty(), "Display 输出不应为空：{err:?}");
        }
    }
}
