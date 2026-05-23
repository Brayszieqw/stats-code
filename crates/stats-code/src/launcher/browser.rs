//! 浏览器调起：在 Windows 上通过 `cmd /c start <url>` 让系统默认浏览器
//! 打开给定 URL；当 `--no-browser` 旗标启用时改为仅向调用方提供的 writer
//! 写入 URL，不调用任何外部命令。
//!
//! 设计参考 `.kiro/specs/single-command-launcher/design.md`，对应
//! Requirements:
//! - 5.1 浏览器自动打开实际地址
//! - 5.3 `--no-browser` 时仅向 stdout 打印 URL
//!
//! 本模块通过 [`Spawner`] trait 把「真正去 spawn 子进程」这一步抽象出来，
//! 默认实现 [`CmdStartSpawner`] 在 Windows 上执行 `cmd /c start <url>`，
//! 而单元测试（task 5.3）可注入一个 mock 实现来断言「`--no-browser` 下
//! spawn 0 次」之类的属性，不需要真正去拉起浏览器。

use std::ffi::OsStr;
use std::io::{self, Write};
use std::process::Command;

/// 启动浏览器进程的抽象层。
///
/// 真实实现 [`CmdStartSpawner`] 在 Windows 上执行 `cmd /c start <url>`；
/// 测试代码可实现一个记录调用次数的 mock，从而在不接触系统浏览器的前提下
/// 验证 [`open`] 的行为。
pub trait Spawner {
    /// 用系统默认浏览器打开 `url`。
    ///
    /// # Errors
    /// 在 spawn 子进程失败时返回对应的 [`io::Error`]。
    fn spawn(&self, url: &str) -> io::Result<()>;
}

/// 默认的 [`Spawner`] 实现：在 Windows 上调用 `cmd /c start <url>`。
///
/// 选择 `cmd /c start` 而非 `ShellExecuteW` 是为了：
/// 1. 避免引入 `windows-sys` / `winapi` 依赖只为打开浏览器；
/// 2. 与设计文档中 `browser::open` 的 sequence 注释保持一致；
/// 3. `start` 内置命令会按用户的「默认浏览器」关联打开，无需自己解析协议。
///
/// 注意 `start` 的第一个引号参数会被 `cmd` 识别为窗口标题，因此这里显式
/// 传入一个空标题 `""`，再传 URL 作为目标，避免 URL 中含空格 / 特殊字符
/// 时被错认成标题。
#[derive(Debug, Default, Clone, Copy)]
pub struct CmdStartSpawner;

impl Spawner for CmdStartSpawner {
    fn spawn(&self, url: &str) -> io::Result<()> {
        spawn_cmd_start(url)
    }
}

#[cfg(target_os = "windows")]
fn spawn_cmd_start(url: &str) -> io::Result<()> {
    // `cmd /c start "" <url>`：第一个 `""` 占位窗口标题，避免 URL 被当成标题。
    Command::new("cmd")
        .args([OsStr::new("/c"), OsStr::new("start"), OsStr::new("")])
        .arg(url)
        .spawn()
        .map(|_child| ())
}

#[cfg(not(target_os = "windows"))]
fn spawn_cmd_start(_url: &str) -> io::Result<()> {
    // 本特性只支持 Windows x64（见 requirements.md Introduction）。在非
    // Windows 平台返回 `Unsupported`，让 launcher 主流程能透传错误而不是
    // 静默成功。这条分支主要用于让 `cargo check` 在开发者的非 Windows
    // 环境下也能跑通。
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "stats-code launcher 仅支持 Windows；当前平台无法调用系统默认浏览器",
    ))
}

/// 通过系统默认浏览器打开 `url`，或在 `no_browser == true` 时改为向 `out`
/// 写入 URL。
///
/// 这是 launcher 主流程在 task 8.1 中调用的入口。本函数内部固定使用
/// [`CmdStartSpawner`]；若需要注入自定义 [`Spawner`]（典型场景为单元测试，
/// 见 task 5.3），请直接调用 [`open_with`]。
///
/// # 行为
/// - `no_browser == false`：调用默认 [`Spawner::spawn`]，不向 `out` 写入。
/// - `no_browser == true`：跳过 spawn，向 `out` 写入 `<url>\n`。
///
/// # Errors
/// - `cmd /c start` spawn 失败时返回对应的 [`io::Error`]；
/// - 写入 `out` 失败时透传 writer 报告的 [`io::Error`]。
pub fn open(url: &str, no_browser: bool, out: &mut dyn Write) -> io::Result<()> {
    open_with(&CmdStartSpawner, url, no_browser, out)
}

/// [`open`] 的可注入版本：把 [`Spawner`] 作为参数传入，便于测试。
///
/// 与 [`open`] 行为完全一致，只是 spawner 由调用方提供。
///
/// # Errors
/// 同 [`open`]。
pub fn open_with<S: Spawner + ?Sized>(
    spawner: &S,
    url: &str,
    no_browser: bool,
    out: &mut dyn Write,
) -> io::Result<()> {
    if no_browser {
        // Requirement 5.3：仅打印 URL，不调用浏览器。
        writeln!(out, "{url}")?;
        return Ok(());
    }
    // Requirement 5.1：默认调起系统浏览器。
    spawner.spawn(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// 测试用的 [`Spawner`] mock：把每次 spawn 的 URL 收集到内部
    /// vector，便于断言「调用次数」与「调用参数」。
    #[derive(Default)]
    struct RecordingSpawner {
        calls: RefCell<Vec<String>>,
    }

    impl Spawner for RecordingSpawner {
        fn spawn(&self, url: &str) -> io::Result<()> {
            self.calls.borrow_mut().push(url.to_string());
            Ok(())
        }
    }

    #[test]
    fn no_browser_writes_url_and_skips_spawn() {
        let spawner = RecordingSpawner::default();
        let mut out: Vec<u8> = Vec::new();
        open_with(
            &spawner,
            "http://127.0.0.1:8080/",
            true,
            &mut out as &mut dyn Write,
        )
        .expect("open_with should succeed in --no-browser mode");

        assert_eq!(
            spawner.calls.borrow().len(),
            0,
            "spawner must not be invoked when no_browser=true"
        );
        let printed = String::from_utf8(out).expect("stdout buffer is valid UTF-8");
        assert_eq!(printed, "http://127.0.0.1:8080/\n");
    }

    #[test]
    fn browser_mode_invokes_spawner_with_url_and_writes_nothing() {
        let spawner = RecordingSpawner::default();
        let mut out: Vec<u8> = Vec::new();
        open_with(
            &spawner,
            "http://127.0.0.1:8081/",
            false,
            &mut out as &mut dyn Write,
        )
        .expect("open_with should succeed in browser mode");

        assert_eq!(
            spawner.calls.borrow().as_slice(),
            &["http://127.0.0.1:8081/".to_string()],
            "spawner must be called exactly once with the given URL"
        );
        assert!(
            out.is_empty(),
            "stdout writer must be untouched in browser mode, got {out:?}"
        );
    }

    #[test]
    fn spawner_error_is_propagated() {
        struct FailingSpawner;
        impl Spawner for FailingSpawner {
            fn spawn(&self, _url: &str) -> io::Result<()> {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
            }
        }
        let mut out: Vec<u8> = Vec::new();
        let err = open_with(
            &FailingSpawner,
            "http://127.0.0.1:8080/",
            false,
            &mut out as &mut dyn Write,
        )
        .expect_err("spawn failure must propagate");
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }
}
