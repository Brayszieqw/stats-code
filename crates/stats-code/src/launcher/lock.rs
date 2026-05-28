//! `Lock_File` 单实例守护骨架。
//!
//! 本文件目前实现 `LockFileV1` schema 与其 (de)serialize 助手（task 3.2）；
//! `try_acquire` / `write_running` / RAII drop 行为在 task 3.5、3.6 中补齐，
//! 此处保留占位以便其他模块在尚未接线的情况下编译通过。

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// `Lock_File` 当前 schema 版本号。读到不一致时视为 stale。
pub const LOCK_SCHEMA_VERSION: u32 = 1;

/// `Lock_File` 持久化结构体（详见 design.md Data Models 节）。
///
/// 字段语义：
/// - `schema_version`：当前固定为 [`LOCK_SCHEMA_VERSION`]，反序列化时不匹配视为
///   stale；
/// - `pid`：写入 lock 的进程 `std::process::id()`；
/// - `url`：可被浏览器直接打开的完整 URL（含 trailing slash）；
/// - `started_at`：诊断用 ISO-8601 时间戳字符串，不参与 stale 判定，因此用
///   `String` 而不引入 `chrono` 依赖；
/// - `mode`：`"prod"` 或 `"dev"`，仅作诊断。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockFileV1 {
    pub schema_version: u32,
    pub pid: u32,
    pub url: String,
    pub started_at: String,
    pub mode: String,
}

impl LockFileV1 {
    /// 按当前 schema 版本构造一条 `Lock_File` 记录。
    #[must_use]
    pub fn new(
        pid: u32,
        url: impl Into<String>,
        started_at: impl Into<String>,
        mode: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: LOCK_SCHEMA_VERSION,
            pid,
            url: url.into(),
            started_at: started_at.into(),
            mode: mode.into(),
        }
    }

    /// 序列化为单行紧凑 JSON 字符串。落盘时由 `write_running` 调用。
    ///
    /// # Errors
    /// 仅在 `serde_json` 内部错误（实际上 `LockFileV1` 不含会触发失败的字段，
    /// 任何错误都视为 I/O 异常）发生时返回 [`LockError::Io`]。
    pub fn to_json(&self) -> Result<String, LockError> {
        serde_json::to_string(self).map_err(|err| {
            LockError::Io(io::Error::other(err))
        })
    }

    /// 从 `Lock_File` 文本反序列化。
    ///
    /// 本函数把以下两类失败统一映射为 [`LockError::ParseStale`]：
    /// - JSON 语法错误（文件内容损坏）；
    /// - 反序列化成功但 `schema_version` 与 [`LOCK_SCHEMA_VERSION`] 不匹配。
    ///
    /// 调用方（`try_acquire`，task 3.5）据此决定删除文件并继续启动。
    ///
    /// # Errors
    /// 见上文：解析失败或版本不匹配时返回 [`LockError::ParseStale`]。
    pub fn parse(text: &str) -> Result<Self, LockError> {
        let parsed: Self = serde_json::from_str(text)
            .map_err(|err| LockError::ParseStale(format!("malformed lock file: {err}")))?;
        if parsed.schema_version != LOCK_SCHEMA_VERSION {
            return Err(LockError::ParseStale(format!(
                "unsupported lock schema_version {} (expected {})",
                parsed.schema_version, LOCK_SCHEMA_VERSION
            )));
        }
        Ok(parsed)
    }
}

/// `Lock_File` RAII 句柄；drop 时尝试删除自身写入的文件。
///
/// 调用方在获取 `Acquired(handle)` 后应先调用 [`write_running`] 把当前实例
/// 的 PID/URL 写入磁盘，再持有 handle 直到主进程关闭。Drop 时自动删除文件，
/// 释放单实例锁（Requirement 1.6, 8.6）。
#[derive(Debug)]
pub struct LockHandle {
    pub(crate) path: PathBuf,
    /// 标记是否已成功写入 lock file 内容；drop 时仅在已写入的前提下删除。
    written: bool,
}

impl LockHandle {
    /// 把当前运行实例的信息写入 `Lock_File（落盘` JSON）。
    ///
    /// # Errors
    /// 当写入文件系统失败时返回 [`LockError::Io`]。
    pub fn write_running(&mut self, record: &LockFileV1) -> Result<(), LockError> {
        let json = record.to_json()?;
        std::fs::write(&self.path, json)?;
        self.written = true;
        Ok(())
    }

    /// 返回此句柄管理的 lock file 路径。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for LockHandle {
    fn drop(&mut self) {
        if self.written {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// `try_acquire` 的三种结果。
#[derive(Debug)]
pub enum AcquireOutcome {
    /// 成功获得锁。
    Acquired(LockHandle),
    /// 已存在存活实例，调用方应改为打开该 URL 后退出。
    Existing { url: String, pid: u32 },
}

/// `Lock_File` 操作错误。
#[derive(Debug)]
pub enum LockError {
    /// 与 `%APPDATA%` 文件系统交互失败。
    Io(io::Error),
    /// `Lock_File` 内容损坏 / `schema_version` 不匹配，应视为 stale 处理。
    ParseStale(String),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "lock file I/O error: {err}"),
            Self::ParseStale(reason) => write!(f, "stale lock file: {reason}"),
        }
    }
}

impl std::error::Error for LockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::ParseStale(_) => None,
        }
    }
}

impl From<io::Error> for LockError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// 尝试获取 `Lock_File`。
///
/// 行为（与 `design.md「Lock_File` 状态机」对齐）：
/// - 文件不存在 → `Acquired(handle)`
/// - 解析失败（JSON 损坏 / `schema_version` 不匹配）→ 删除并 `Acquired(handle)`
/// - 解析成功且 alive → `Existing { url, pid }`
/// - 解析成功但 stale（pid 不存活或端口不可达）→ 删除并 `Acquired(handle)`
///
/// `pid_alive_fn` 与 `port_open_fn` 以闭包注入，让测试无需真正探测 OS 进程/端口。
///
/// # Errors
/// 当文件系统 I/O 因权限不足等原因失败时返回 [`LockError::Io`]。
pub fn try_acquire<F, G>(
    path: &Path,
    _current_pid: u32,
    pid_alive_fn: F,
    port_open_fn: G,
) -> Result<AcquireOutcome, LockError>
where
    F: FnOnce(u32) -> bool,
    G: FnOnce(&str) -> bool,
{
    // 1. 文件不存在 → 直接获取。
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(AcquireOutcome::Acquired(LockHandle {
                path: path.to_path_buf(),
                written: false,
            }));
        }
        Err(e) => return Err(LockError::Io(e)),
    };

    // 2. 解析失败 → stale，删除并获取。
    let parsed = match LockFileV1::parse(&content) {
        Ok(p) => p,
        Err(LockError::ParseStale(_)) => {
            let _ = std::fs::remove_file(path);
            return Ok(AcquireOutcome::Acquired(LockHandle {
                path: path.to_path_buf(),
                written: false,
            }));
        }
        Err(e) => return Err(e),
    };

    // 3. 判活：pid_alive ∧ port_open。
    let alive = is_lock_alive_with(
        || pid_alive_fn(parsed.pid),
        || port_open_fn(&parsed.url),
    );

    if alive {
        // 存活的实例，调用方应改为打开其 URL。
        Ok(AcquireOutcome::Existing {
            url: parsed.url,
            pid: parsed.pid,
        })
    } else {
        // stale → 删除并获取。
        let _ = std::fs::remove_file(path);
        Ok(AcquireOutcome::Acquired(LockHandle {
            path: path.to_path_buf(),
            written: false,
        }))
    }
}

/// 纯逻辑判定一条 `Lock_File` 记录是否仍然存活（布尔输入版本）。
///
/// 真值表：`pid_alive ∧ port_open`，即两个证据缺一就视为 stale（详见
/// design.md「单实例只信『PID + 端口可达』联合证据」与 Requirement 8.2）。
///
/// 该重载用于已经把两个证据求值完毕的调用点；若需要把昂贵的端口探测延迟到
/// PID 仍然存活时才执行，请改用 [`is_lock_alive_with`]。
#[must_use]
pub fn is_lock_alive(pid_alive: bool, port_open: bool) -> bool {
    pid_alive && port_open
}

/// 高阶版 `is_lock_alive`：通过注入两个闭包延迟求值，便于在测试中 mock。
///
/// 与 [`is_lock_alive`] 保持相同的真值表 `pid_alive ∧ port_open`，但显式以
/// **短路** 方式求值——`port_open` 仅在 `pid_alive` 已返回 `true` 时才调用。
/// 这与 design.md 中给出的「先验 PID 再探端口」判定顺序一致，也避免了在
/// 进程已退出的常见场景下对死端口发起多余探测。
///
/// 由 task 3.5 中的 `try_acquire` 调用：
/// ```ignore
/// let alive = is_lock_alive_with(
///     || sysinfo_pid_alive(parsed.pid),
///     || tcp_connect(&parsed.url).is_ok(),
/// );
/// ```
#[must_use]
pub fn is_lock_alive_with<F, G>(pid_alive: F, port_open: G) -> bool
where
    F: FnOnce() -> bool,
    G: FnOnce() -> bool,
{
    pid_alive() && port_open()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> LockFileV1 {
        LockFileV1::new(
            18432,
            "http://127.0.0.1:8080/",
            "2025-01-15T10:23:11Z",
            "prod",
        )
    }

    #[test]
    fn new_sets_current_schema_version() {
        let lf = sample();
        assert_eq!(lf.schema_version, LOCK_SCHEMA_VERSION);
        assert_eq!(lf.pid, 18432);
        assert_eq!(lf.url, "http://127.0.0.1:8080/");
        assert_eq!(lf.started_at, "2025-01-15T10:23:11Z");
        assert_eq!(lf.mode, "prod");
    }

    #[test]
    fn round_trip_preserves_all_fields() {
        let lf = sample();
        let serialized = lf.to_json().expect("serialize ok");
        let parsed = LockFileV1::parse(&serialized).expect("parse ok");
        assert_eq!(parsed, lf);
    }

    #[test]
    fn serialized_json_matches_design_schema_keys() {
        let lf = sample();
        let serialized = lf.to_json().expect("serialize ok");
        // 设计文档明确指定的 5 个键名必须出现，避免后续重命名打破契约。
        for key in [
            "\"schema_version\"",
            "\"pid\"",
            "\"url\"",
            "\"started_at\"",
            "\"mode\"",
        ] {
            assert!(
                serialized.contains(key),
                "expected key {key} in serialized lock file: {serialized}"
            );
        }
    }

    #[test]
    fn parse_rejects_mismatched_schema_version_as_stale() {
        let payload = r#"{
            "schema_version": 999,
            "pid": 1,
            "url": "http://127.0.0.1:8080/",
            "started_at": "2025-01-15T10:23:11Z",
            "mode": "prod"
        }"#;
        let err = LockFileV1::parse(payload).expect_err("must be stale");
        match err {
            LockError::ParseStale(msg) => assert!(
                msg.contains("999"),
                "expected diagnostic to mention version 999, got {msg}"
            ),
            other => panic!("expected ParseStale, got {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_malformed_json_as_stale() {
        let err = LockFileV1::parse("{not json").expect_err("must be stale");
        assert!(
            matches!(err, LockError::ParseStale(_)),
            "expected ParseStale, got {err:?}"
        );
    }

    #[test]
    fn parse_accepts_valid_v1_payload() {
        let payload = r#"{
            "schema_version": 1,
            "pid": 42,
            "url": "http://127.0.0.1:8081/",
            "started_at": "2025-01-15T10:23:11Z",
            "mode": "dev"
        }"#;
        let parsed = LockFileV1::parse(payload).expect("valid v1");
        assert_eq!(parsed.pid, 42);
        assert_eq!(parsed.url, "http://127.0.0.1:8081/");
        assert_eq!(parsed.mode, "dev");
    }

    // --- is_lock_alive / is_lock_alive_with: 真值表 (Requirement 8.2) -------

    #[test]
    fn is_lock_alive_truth_table_flat() {
        // pid_alive ∧ port_open
        assert!(is_lock_alive(true, true));
        assert!(!is_lock_alive(true, false));
        assert!(!is_lock_alive(false, true));
        assert!(!is_lock_alive(false, false));
    }

    #[test]
    fn is_lock_alive_with_truth_table_via_closures() {
        // 同样的真值表，但走 FnOnce 闭包注入，证明高阶版与布尔版一致。
        for &pid in &[true, false] {
            for &port in &[true, false] {
                let got = is_lock_alive_with(|| pid, || port);
                assert_eq!(
                    got,
                    pid && port,
                    "is_lock_alive_with({pid}, {port}) should be {}",
                    pid && port
                );
            }
        }
    }

    #[test]
    fn is_lock_alive_with_short_circuits_when_pid_dead() {
        use std::cell::Cell;

        // PID 已不存活时 port 探测必须被跳过：避免对死端口浪费 TCP 探测。
        let port_probe_called = Cell::new(false);
        let alive = is_lock_alive_with(
            || false,
            || {
                port_probe_called.set(true);
                true
            },
        );

        assert!(!alive);
        assert!(
            !port_probe_called.get(),
            "port_open closure must not be invoked when pid_alive is false"
        );
    }

    #[test]
    fn is_lock_alive_with_invokes_port_probe_when_pid_alive() {
        use std::cell::Cell;

        // PID 存活时 port 探测必须被执行（结果决定最终 bool）。
        let port_probe_called = Cell::new(false);
        let alive = is_lock_alive_with(
            || true,
            || {
                port_probe_called.set(true);
                false
            },
        );

        assert!(!alive);
        assert!(
            port_probe_called.get(),
            "port_open closure must be invoked once pid_alive returned true"
        );
    }

    // --- try_acquire: 四种场景 (Requirement 8.1, 8.3, 8.4) ---

    #[test]
    fn try_acquire_no_file_returns_acquired() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock_path = dir.path().join("running.lock");

        let outcome = try_acquire(&lock_path, 1234, |_| false, |_| false)
            .expect("should not error");
        assert!(
            matches!(outcome, AcquireOutcome::Acquired(_)),
            "expected Acquired when lock file does not exist"
        );
    }

    #[test]
    fn try_acquire_malformed_file_returns_acquired() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock_path = dir.path().join("running.lock");
        std::fs::write(&lock_path, "not valid json").unwrap();

        let outcome = try_acquire(&lock_path, 1234, |_| true, |_| true)
            .expect("should not error");
        assert!(
            matches!(outcome, AcquireOutcome::Acquired(_)),
            "expected Acquired when lock file is malformed"
        );
        assert!(!lock_path.exists(), "malformed lock file should be deleted");
    }

    #[test]
    fn try_acquire_alive_instance_returns_existing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock_path = dir.path().join("running.lock");
        let lf = LockFileV1::new(9999, "http://127.0.0.1:8080/", "2025-01-15T10:00:00Z", "prod");
        std::fs::write(&lock_path, lf.to_json().unwrap()).unwrap();

        let outcome = try_acquire(&lock_path, 1234, |pid| pid == 9999, |_| true)
            .expect("should not error");
        match outcome {
            AcquireOutcome::Existing { url, pid } => {
                assert_eq!(pid, 9999);
                assert_eq!(url, "http://127.0.0.1:8080/");
            }
            _ => panic!("expected Existing, got {outcome:?}"),
        }
        assert!(lock_path.exists(), "alive lock file should remain");
    }

    #[test]
    fn try_acquire_stale_pid_dead_returns_acquired() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock_path = dir.path().join("running.lock");
        let lf = LockFileV1::new(9999, "http://127.0.0.1:8080/", "2025-01-15T10:00:00Z", "prod");
        std::fs::write(&lock_path, lf.to_json().unwrap()).unwrap();

        // pid dead → stale regardless of port
        let outcome = try_acquire(&lock_path, 1234, |_| false, |_| true)
            .expect("should not error");
        assert!(
            matches!(outcome, AcquireOutcome::Acquired(_)),
            "expected Acquired when pid is dead (stale)"
        );
        assert!(!lock_path.exists(), "stale lock file should be deleted");
    }

    #[test]
    fn try_acquire_stale_port_closed_returns_acquired() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock_path = dir.path().join("running.lock");
        let lf = LockFileV1::new(9999, "http://127.0.0.1:8080/", "2025-01-15T10:00:00Z", "prod");
        std::fs::write(&lock_path, lf.to_json().unwrap()).unwrap();

        // pid alive but port closed → stale
        let outcome = try_acquire(&lock_path, 1234, |_| true, |_| false)
            .expect("should not error");
        assert!(
            matches!(outcome, AcquireOutcome::Acquired(_)),
            "expected Acquired when port is closed (stale)"
        );
        assert!(!lock_path.exists(), "stale lock file should be deleted");
    }

    // --- LockHandle RAII: write_running + drop-on-delete (Requirement 1.6, 8.6) ---

    #[test]
    fn lock_handle_write_running_creates_valid_json_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock_path = dir.path().join("running.lock");

        let outcome = try_acquire(&lock_path, 42, |_| false, |_| false)
            .expect("should not error");
        let mut handle = match outcome {
            AcquireOutcome::Acquired(h) => h,
            other => panic!("expected Acquired, got {other:?}"),
        };

        let record = LockFileV1::new(42, "http://127.0.0.1:8081/", "2025-06-01T12:00:00Z", "dev");
        handle.write_running(&record).expect("write should succeed");

        assert!(lock_path.exists(), "lock file should exist after write_running");
        let content = std::fs::read_to_string(&lock_path).unwrap();
        let parsed = LockFileV1::parse(&content).expect("should parse back");
        assert_eq!(parsed, record);

        // drop deletes the file
        drop(handle);
        assert!(!lock_path.exists(), "lock file should be deleted after drop");
    }

    #[test]
    fn lock_handle_drop_without_write_does_not_delete() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock_path = dir.path().join("running.lock");
        // Pre-create a file to simulate someone else's data
        std::fs::write(&lock_path, "someone else").unwrap();

        // Acquire returns a handle for a non-existent path but we test with
        // a path that has existing content and written=false
        let handle = LockHandle {
            path: lock_path.clone(),
            written: false,
        };
        drop(handle);
        // File should still exist because we never called write_running
        assert!(lock_path.exists(), "lock file should NOT be deleted when write_running was never called");
    }
}
