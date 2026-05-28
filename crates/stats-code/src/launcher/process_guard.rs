//! Windows Job Object 进程守护。
//!
//! 在 dev 模式下 launcher 会 spawn `npm run dev` 作为 `Vite_Dev_Server` 子进程，
//! 本模块负责把该子进程绑定到一个带 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`
//! 的 Job Object 上。当 launcher 主进程因任何原因（正常退出 / panic /
//! Ctrl+C / SIGKILL）退出时，内核会同步关闭进程的所有句柄，进而触发 Job
//! Object 的「关闭即杀全部成员」语义，确保 Vite 子进程及其孙进程在主进程
//! 退出后的 5 秒内必死（Requirement 7.3）。
//!
//! 跨平台考虑：仅 Windows 提供真实实现；其它平台编译时落到一个 stub，调用
//! [`ProcessGuard::spawn_in_job`] 直接返回 [`GuardError::UnsupportedPlatform`]，
//! 让仓库在 Linux CI 上仍能 `cargo check` 通过（虽然产品形态只发 Windows）。

// 本模块通过 `windows-sys` 直接调用 Win32 Job Object FFI（CreateJobObjectW /
// SetInformationJobObject / AssignProcessToJobObject / CloseHandle），无法用
// 安全包装的 crate 替代（`std::process` 暴露不到 Job Object 语义）。这是 workspace
// 级 `unsafe_code = "deny"` 政策中「在受控位置 #[allow(...)] 局部启用」的预期点。
#![allow(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

use std::io;
use std::process::Command;

use thiserror::Error;

#[cfg(windows)]
use std::process::Child;

#[cfg(windows)]
use windows_sys::Win32::Foundation::HANDLE;

/// Job Object 守护错误。
#[derive(Debug, Error)]
pub enum GuardError {
    /// `CreateJobObjectW` 失败。
    #[error("CreateJobObjectW failed: {0}")]
    JobCreate(#[source] io::Error),
    /// `SetInformationJobObject` 设置 `KILL_ON_JOB_CLOSE` 失败。
    #[error("SetInformationJobObject failed: {0}")]
    SetInformation(#[source] io::Error),
    /// 子进程 spawn 失败。
    #[error("spawn child process failed: {0}")]
    Spawn(#[source] io::Error),
    /// `AssignProcessToJobObject` 失败。
    #[error("AssignProcessToJobObject failed: {0}")]
    Assign(#[source] io::Error),
    /// 关闭 Job Object 句柄时失败。
    #[error("CloseHandle on job object failed: {0}")]
    Close(#[source] io::Error),
    /// 在非 Windows 平台调用时返回，便于仓库在 Linux CI 上仍能编译。
    #[error("ProcessGuard is only supported on Windows")]
    UnsupportedPlatform,
}

// ---------------------------------------------------------------------------
// Windows 真实实现
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub struct ProcessGuard {
    /// Job Object 句柄。drop `时关闭，关闭即杀子进程（KILL_ON_JOB_CLOSE`）。
    /// `kill()` 主动回收时会先把它置空避免 Drop 二次 Close。
    job: HANDLE,
    /// 已 spawn 的子进程；持有它以便提供 PID 与显式 kill。
    child: Child,
}

#[cfg(windows)]
impl std::fmt::Debug for ProcessGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessGuard")
            .field("job", &(self.job as usize))
            .field("child_pid", &self.child.id())
            .finish()
    }
}

// HANDLE 是 *mut c_void，默认 !Send + !Sync。Job Object 句柄本质是不透明
// 内核对象 ID，跨线程移动 / 共享读取均安全。FrontendHandle::DevVite 需要
// ProcessGuard 实现 Send 以便在 tokio task 间转移所有权。
#[cfg(windows)]
unsafe impl Send for ProcessGuard {}
#[cfg(windows)]
unsafe impl Sync for ProcessGuard {}

#[cfg(windows)]
impl ProcessGuard {
    /// 在新建的 Job Object 中启动一个子进程。
    ///
    /// 步骤：
    /// 1. `CreateJobObjectW` 创建匿名 Job Object。
    /// 2. `SetInformationJobObject(JobObjectExtendedLimitInformation,
    ///    LimitFlags=JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)` 设定关闭即杀。
    /// 3. `cmd.spawn()` 启动子进程。
    /// 4. `AssignProcessToJobObject` 把子进程绑入 job。
    ///
    /// 任一步失败会回滚已分配资源（关闭 job 句柄 / 杀掉已 spawn 的子进程）
    /// 并返回对应的 [`GuardError`] 变体。
    ///
    /// # Errors
    ///
    /// - [`GuardError::JobCreate`]：`CreateJobObjectW` 返回 NULL。
    /// - [`GuardError::SetInformation`]：`SetInformationJobObject` 返回 FALSE。
    /// - [`GuardError::Spawn`]：底层 `Command::spawn` 失败。
    /// - [`GuardError::Assign`]：`AssignProcessToJobObject` 返回 FALSE。
    pub fn spawn_in_job(mut cmd: Command) -> Result<Self, GuardError> {
        use std::os::windows::io::AsRawHandle;
        use std::ptr;

        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };

        // 1. CreateJobObjectW(NULL, NULL) — 匿名、默认安全描述符。
        // SAFETY: 两个参数均允许 NULL；返回值若为 NULL 则代表失败，由
        // GetLastError() 通过 io::Error::last_os_error() 取错误码。
        let job: HANDLE = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if job.is_null() {
            return Err(GuardError::JobCreate(io::Error::last_os_error()));
        }

        // 2. 设置 KILL_ON_JOB_CLOSE 限制。
        // SAFETY: zeroed 在该 POD 结构体上是合法初始化（全字段为整数 / 句柄）。
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: `info` 在栈上对齐且大小匹配，传入指针 + 字节数即可。
        let ok = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&info).cast::<core::ffi::c_void>(),
                u32::try_from(std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
                    .expect("JOBOBJECT_EXTENDED_LIMIT_INFORMATION size fits in u32"),
            )
        };
        if ok == 0 {
            let err = io::Error::last_os_error();
            // SAFETY: job 由 CreateJobObjectW 返回，未关闭过。
            unsafe { CloseHandle(job) };
            return Err(GuardError::SetInformation(err));
        }

        // 3. spawn 子进程。
        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                // SAFETY: 同上，job 仍持有有效句柄。
                unsafe { CloseHandle(job) };
                return Err(GuardError::Spawn(e));
            }
        };

        // 4. 把子进程绑入 job。child.as_raw_handle() 返回 std 的 RawHandle，
        //    其与 windows-sys 的 HANDLE 都是 *mut c_void，可直接 cast。
        let process_handle: HANDLE = child.as_raw_handle().cast();
        // SAFETY: 两个句柄都来自即时调用，确保有效。
        let ok = unsafe { AssignProcessToJobObject(job, process_handle) };
        if ok == 0 {
            let err = io::Error::last_os_error();
            // 子进程已 spawn，job 未生效。先杀子进程再关 job 防泄漏。
            // 用 mut 绑定让 kill / wait 能调用。
            let mut child = child;
            let _ = child.kill();
            let _ = child.wait();
            unsafe { CloseHandle(job) };
            return Err(GuardError::Assign(err));
        }

        Ok(Self { job, child })
    }

    /// 主动杀死子进程并关闭 Job Object 句柄。
    ///
    /// 关闭 Job Object 会触发 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`，把所有
    /// 仍属于该 job 的进程同步终止。随后 `wait()` 子进程的 Child handle 以
    /// 完成 reap，避免 zombie。
    ///
    /// # Errors
    ///
    /// 返回 [`GuardError::Close`] 当 `CloseHandle` 在 job 句柄上失败时。
    pub fn kill(mut self) -> Result<(), GuardError> {
        use windows_sys::Win32::Foundation::CloseHandle;

        let job = std::mem::replace(&mut self.job, std::ptr::null_mut());
        // SAFETY: job 来自 spawn_in_job 的 CreateJobObjectW 返回值，未关闭过；
        // null 化已在上面完成，Drop 不会再 Close 一次。
        let ok = unsafe { CloseHandle(job) };
        // 不论 CloseHandle 成败，都尝试 reap 子进程（已被 OS 杀掉）。
        let _ = self.child.wait();
        if ok == 0 {
            return Err(GuardError::Close(io::Error::last_os_error()));
        }
        Ok(())
    }

    /// 返回受守护的子进程的 PID。
    #[must_use]
    pub fn child_pid(&self) -> u32 {
        self.child.id()
    }

    /// 取走子进程 stdout 管道的所有权（仅当 spawn 前调用方为 `Command`
    /// 设置了 [`std::process::Stdio::piped`] 时可用）。
    ///
    /// 提供该最小访问器是为了让 [`crate::launcher::frontend::ensure_frontend`]
    /// 在 `dev-vite` 分支中扫描 Vite 启动日志（"Local:" / "ready in"）以判定
    /// 就绪，而无需把整个 [`std::process::Child`] 暴露出去。一次取走后再
    /// 调用返回 `None`。
    pub fn child_stdout_take(&mut self) -> Option<std::process::ChildStdout> {
        self.child.stdout.take()
    }

    /// 取走子进程 stderr 管道的所有权；语义同 [`Self::child_stdout_take`]。
    ///
    /// 用于在 dev 分支中起一个排空线程，避免 stderr 管道写满阻塞 Vite。
    pub fn child_stderr_take(&mut self) -> Option<std::process::ChildStderr> {
        self.child.stderr.take()
    }

    /// 阻塞等待子进程退出，返回退出状态。
    ///
    /// 用于 task 15.2 中的 Vite 早退监听：调用方在独立线程中调用本方法，
    /// 当 Vite 子进程异常退出时通知主线程触发优雅关闭。
    ///
    /// # Errors
    /// 当底层 `child.wait()` 失败时返回 OS 错误。
    pub fn wait_child(&mut self) -> io::Result<std::process::ExitStatus> {
        self.child.wait()
    }
}

#[cfg(windows)]
impl Drop for ProcessGuard {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;

        if !self.job.is_null() {
            // SAFETY: 句柄非空意味着 kill() 未被调用过，此处是首次也是唯一
            // 一次关闭。Close 即杀掉子进程（KILL_ON_JOB_CLOSE），子进程的
            // Child handle 会在 Drop 中被简单丢弃；OS 在父进程退出时回收。
            unsafe { CloseHandle(self.job) };
            self.job = std::ptr::null_mut();
        }
    }
}

// ---------------------------------------------------------------------------
// 非 Windows stub：仅保证编译通过，运行时立即报告 UnsupportedPlatform
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
#[derive(Debug)]
pub struct ProcessGuard {
    /// 永远无法构造：非 Windows 路径下 `spawn_in_job` 始终返回 Err。
    _never: std::convert::Infallible,
}

#[cfg(not(windows))]
impl ProcessGuard {
    /// 非 Windows 平台 stub：始终返回 [`GuardError::UnsupportedPlatform`]，
    /// 让 Linux CI 等环境能完成 `cargo check` 但运行时拒绝误用。
    ///
    /// # Errors
    ///
    /// 总是返回 [`GuardError::UnsupportedPlatform`]。
    pub fn spawn_in_job(_cmd: Command) -> Result<Self, GuardError> {
        Err(GuardError::UnsupportedPlatform)
    }

    /// 非 Windows 平台 stub：永远不可达，因为 `Self` 无法构造。
    #[allow(clippy::missing_errors_doc, clippy::unused_self)]
    pub fn kill(self) -> Result<(), GuardError> {
        match self._never {}
    }

    /// 非 Windows 平台 stub：永远不可达。
    #[must_use]
    #[allow(clippy::unused_self)]
    pub fn child_pid(&self) -> u32 {
        match self._never {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn spawn_in_job_returns_unsupported_on_non_windows() {
        let err = ProcessGuard::spawn_in_job(Command::new("true"))
            .err()
            .expect("non-Windows stub must Err");
        assert!(matches!(err, GuardError::UnsupportedPlatform));
    }

    #[cfg(windows)]
    #[test]
    fn spawn_in_job_then_kill_roundtrip() {
        // 用 cmd /c exit 0 作最低成本的 spawn 标的；它会立即退出，
        // 验证 Job Object 创建 / Assign 链路无错。
        let mut cmd = Command::new("cmd");
        cmd.args(["/c", "exit 0"]);
        let guard =
            ProcessGuard::spawn_in_job(cmd).expect("spawn_in_job should succeed on Windows");
        assert!(guard.child_pid() > 0, "child PID should be non-zero");
        guard.kill().expect("kill should succeed");
    }

    #[cfg(windows)]
    #[test]
    fn spawn_failure_propagates_as_guard_error() {
        // 不存在的可执行文件 → cmd.spawn() 返回 Err，由 spawn_in_job 包成
        // GuardError::Spawn，并且不应泄漏 Job Object 句柄（无法在单元测试
        // 内直接断言句柄计数，但能至少确认错误变体正确）。
        let cmd = Command::new("c:\\__definitely_not_a_real_binary_for_pg_test__.exe");
        let err = ProcessGuard::spawn_in_job(cmd)
            .err()
            .expect("spawn must fail when binary does not exist");
        assert!(matches!(err, GuardError::Spawn(_)), "got: {err:?}");
    }
}
