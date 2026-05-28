//! `SpawnPolicy` 哨兵：禁止 sidecar / SPA 渲染 / snapshot 导出三条路径
//! 触达 R / SAS / Python / SPSS 外部运行时。
//!
//! Feature: parity-and-multilang-sidecar — task 2.6.
//!
//! 本模块给三条 wave-1 流水线提供一道运行时闸门，对应：
//!
//! * Requirement 10.1 — sidecar snippet 生成、SPA 渲染、Audit Snapshot 导出
//!   过程中，禁止 spawn `{R, Rscript, python, python3, pythonw, sas, spss,
//!   pspp, pspp-cli, statistics, stats}` 进程，也禁止 `libloading` 加载这
//!   些运行时独占的共享库。
//! * Requirement 10.2 — 在不安装上述任何参考软件的宿主机上，三条流水线
//!   仍能完整跑通，因为它们从不依赖外部运行时。
//! * Requirement 10.5 — 一旦命中黑名单立刻返回结构化 `ForbiddenSpawn`
//!   错误、操作整体中止、不留 partial 产物。
//!
//! Launcher 路径**不在** scope 内（Requirement 10.4）：
//! `crates/stats-code/src/launcher/browser.rs` 仍然走自己的 `Spawner` trait，
//! 调起 `cmd /c start <url>` 也不算违例。本模块定义的 `Spawner` 与
//! launcher 中的同名 trait 是两个独立类型，互不影响；如果某个文件同时引用
//! 两边，按模块路径限定即可（`crate::launcher::browser::Spawner` /
//! `crate::spawn_policy::Spawner`）。
//!
//! # 使用范式
//!
//! Task 2.5 (`sidecar::generate_snippet`) 与 task 6.7
//! (`snapshot::export_snapshot`) 把它们的整个调用栈包裹进
//! [`forbid_external_runtimes_scope`]，闭包内部每次潜在的 spawn 之前都先
//! 调用 [`SpawnPolicy::check`] 过一遍。或者把任意 [`Spawner`] 实现传给
//! [`SpawnPolicy::wrap_spawner`] 拿到一个自动短路黑名单的
//! [`ForbidExternalRuntimesGuard`]。
//!
//! # 关于 `libloading::Library::new` 的拦截策略
//!
//! 本 crate 不依赖 `libloading`，且 wave-1 的三条流水线（task 2.5 / 6.7）
//! 都不动态加载共享库；因此本模块不能从外部 wrap `libloading`。我们对外
//! 暴露 [`check_library_load`]：调用方在调用 `libloading::Library::new`
//! 之前必须把库名（绝对路径或裸 SO 名）过一次 [`check_library_load`]，
//! 命中黑名单则返回 [`SpawnError::ForbiddenSpawn`]、调用方据此中止。
//! 这是一个**防御性 scaffold**，并非主动执行点；wave-1 不会触发。
//!
//! # 不变量
//!
//! * [`SpawnPolicy::check`] 不读时钟、不读环境变量、不打开任何 fd、不获取锁；
//!   仅依据 `&self` 中嵌入的常量黑名单与字符串处理函数做出决策。
//! * [`SpawnPolicy::forbid_external_runtimes`] 是常量构造，跨进程返回结构
//!   完全相同的策略实例。
//! * 大小写策略：在 Windows 上对命令名与库名做大小写不敏感比较（Windows
//!   文件系统/PATH 解析本身不区分大小写）；在 Unix 上做**精确大小写**比较
//!   （Unix 文件系统区分大小写，`Rscript` 和 `rscript` 在 Linux 上是两个
//!   不同的可执行文件）。
//! * 路径剥离：比较前先取 basename（去掉 `/` 或 `\\` 之前的目录段）；
//!   在 Windows 上额外去掉尾部 `.exe`（不区分大小写）。

use std::io;

/// 哨兵命中外部运行时时返回的错误。
///
/// `ForbiddenSpawn` 由 [`SpawnPolicy::check`] 与
/// [`ForbidExternalRuntimesGuard::spawn_command`] 产生，`Io` 由
/// [`ProductionSpawner::spawn_command`] 透传 `std::process::Command::spawn`
/// 的 IO 错误。
#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    /// 命中黑名单，操作中止、不留 partial 产物。
    #[error("forbidden spawn '{command}': {reason}")]
    ForbiddenSpawn {
        /// 调用方传入的原始命令字符串（未做规范化）。
        command: String,
        /// 拒绝原因的静态分类标签。
        reason: &'static str,
    },
    /// `std::process::Command::spawn` 报告的底层 IO 错误。
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// 调用方走这个 trait 来 spawn 子进程，便于在测试中注入 mock 并被
/// [`ForbidExternalRuntimesGuard`] 透明监视。
///
/// 注意：本 trait 与 `crate::launcher::browser::Spawner` 同名但**不是**同
/// 一个类型；后者服务于浏览器调起、不在 [`SpawnPolicy`] 的 scope 内。
pub trait Spawner {
    /// 启动一个名为 `command` 的子进程。`command` 的语义与
    /// `std::process::Command::new` 一致：可以是裸命令名（依赖 PATH）或
    /// 绝对路径。
    ///
    /// # Errors
    /// 实现方根据自身职责返回 [`SpawnError::ForbiddenSpawn`]（被
    /// [`SpawnPolicy::check`] 拒绝）或 [`SpawnError::Io`]（底层 spawn 失败）。
    fn spawn_command(&self, command: &str) -> Result<(), SpawnError>;
}

/// 真正调用 `std::process::Command` 的 [`Spawner`]。
///
/// 这是 fallback 默认实现：本 crate 自己**不**直接使用它，sidecar /
/// snapshot 流水线只通过 [`ForbidExternalRuntimesGuard`] 走过来的那一层
/// 间接调用。把它作为公开 API 暴露出来是为了让上游调用方（如未来的
/// agent-server handlers）拿到一个可以包裹的「真实」 spawner。
#[derive(Debug, Default, Clone, Copy)]
pub struct ProductionSpawner;

impl Spawner for ProductionSpawner {
    fn spawn_command(&self, command: &str) -> Result<(), SpawnError> {
        std::process::Command::new(command)
            .spawn()
            .map(|_child| ())
            .map_err(SpawnError::Io)
    }
}

/// 包裹任意 [`Spawner`] 并在每次 spawn 之前先过一遍 [`SpawnPolicy::check`]。
///
/// 命中黑名单时立即返回 [`SpawnError::ForbiddenSpawn`] 并**不会**调用内
/// 层 spawner，从源头杜绝外部统计运行时被拉起。
pub struct ForbidExternalRuntimesGuard<S: Spawner> {
    inner: S,
    policy: SpawnPolicy,
}

impl<S: Spawner> ForbidExternalRuntimesGuard<S> {
    /// 用给定的 inner spawner 与 [`SpawnPolicy::forbid_external_runtimes`]
    /// 默认策略组装一个 guard。
    #[must_use]
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            policy: SpawnPolicy::forbid_external_runtimes(),
        }
    }

    /// 借用底层策略以便 ad-hoc 调用 [`SpawnPolicy::check`]，例如在 spawn
    /// 之外的地方（动态库加载点）做闸门检查。
    #[must_use]
    pub fn policy(&self) -> &SpawnPolicy {
        &self.policy
    }
}

impl<S: Spawner> Spawner for ForbidExternalRuntimesGuard<S> {
    fn spawn_command(&self, command: &str) -> Result<(), SpawnError> {
        self.policy.check(command)?;
        self.inner.spawn_command(command)
    }
}

/// 「禁止外部运行时」哨兵。
///
/// 用 [`SpawnPolicy::forbid_external_runtimes`] 构造一个实例，然后调用
/// [`SpawnPolicy::check`] 在每次 spawn 之前过滤命令名、用
/// [`check_library_load`] 在每次动态加载之前过滤共享库名。
#[derive(Debug, Clone)]
pub struct SpawnPolicy {
    /// 命令名黑名单。`forbid_external_runtimes` 注入的内容来自
    /// Requirement 10.1 列出的外部统计运行时集合。
    commands: &'static [&'static str],
    /// 共享库名黑名单（仅供 [`check_library_load`] 使用，本 crate 自身
    /// 不调用 `libloading`）。
    libraries: &'static [&'static str],
}

/// Requirement 10.1 列举的外部统计运行时命令名集合。
///
/// 顺序无关；[`SpawnPolicy::check`] 内部用线性扫描比对。
const FORBIDDEN_COMMANDS: &[&str] = &[
    "Rscript",
    "R",
    "python",
    "python3",
    "pythonw",
    "sas",
    "spss",
    "pspp",
    "pspp-cli",
    "statistics",
    "stats",
];

/// 与上述命令名对应的共享库黑名单。
///
/// 名单的取舍：
/// * `libR.so` / `libR.dylib` / `R.dll` —— R 在 Linux / macOS / Windows
///   上的主进程动态库；以这些库为入口动态加载 R 等价于在进程内拉起 R。
/// * `libpython3.so` / `libpython3.dylib` / `python3.dll` / `python.dll`
///   —— `CPython` 解释器的嵌入入口。
///
/// 不包含 SAS / SPSS / PSPP 的共享库：这些产品要么不以独立 SO/DLL 形式
/// 暴露 ABI（SAS、SPSS 几乎只通过 IPC），要么 SO 名不稳定到值得做静态
/// 黑名单。如果未来发现需要拦截，按 Requirement 10.5 的契约扩展本数组
/// 即可。
const FORBIDDEN_LIBRARIES: &[&str] = &[
    "libR.so",
    "libR.dylib",
    "R.dll",
    "libpython3.so",
    "libpython3.dylib",
    "python3.dll",
    "python.dll",
];

impl SpawnPolicy {
    /// 构造禁止外部统计运行时的策略实例。
    ///
    /// 这是 Requirement 10.1 的可执行编码：命中
    /// [`FORBIDDEN_COMMANDS`] 或 [`FORBIDDEN_LIBRARIES`] 的调用都会被
    /// [`SpawnPolicy::check`] / [`check_library_load`] 拒绝。
    #[must_use]
    pub fn forbid_external_runtimes() -> Self {
        Self {
            commands: FORBIDDEN_COMMANDS,
            libraries: FORBIDDEN_LIBRARIES,
        }
    }

    /// 检查 `command` 是否命中外部运行时命令黑名单。
    ///
    /// 比较前会先做规范化：取 basename（按 `/` 与 `\\` 拆分）、Windows
    /// 上额外去掉尾部 `.exe`（不区分大小写）。在 Windows 上做大小写不
    /// 敏感比较，在 Unix 上做精确大小写比较 —— 见模块级文档「不变量」一节。
    ///
    /// # Errors
    /// 命中黑名单时返回 [`SpawnError::ForbiddenSpawn`]，`command` 字段
    /// 保留调用方传入的原始字符串（未规范化），便于上层日志直接打印用户
    /// 看到的命令。
    pub fn check(&self, command: &str) -> Result<(), SpawnError> {
        let normalized = normalize_command(command);
        for entry in self.commands {
            if commands_match(&normalized, entry) {
                return Err(SpawnError::ForbiddenSpawn {
                    command: command.to_string(),
                    reason: "external statistical runtime",
                });
            }
        }
        Ok(())
    }

    /// 把 `inner` 包装成一个会自动调用 [`SpawnPolicy::check`] 的
    /// [`ForbidExternalRuntimesGuard`]。
    #[must_use]
    pub fn wrap_spawner<S: Spawner>(self, inner: S) -> ForbidExternalRuntimesGuard<S> {
        ForbidExternalRuntimesGuard {
            inner,
            policy: self,
        }
    }
}

/// 检查动态库名 `name` 是否命中外部运行时共享库黑名单。
///
/// 见模块级文档「关于 `libloading::Library::new` 的拦截策略」一节。本函
/// 数是 wave-1 的防御性 scaffold；调用方在动态加载之前必须主动调用它，
/// 框架本身没有从外部 wrap `libloading` 的途径。
///
/// # Errors
/// 命中 [`FORBIDDEN_LIBRARIES`] 时返回 [`SpawnError::ForbiddenSpawn`]。
pub fn check_library_load(policy: &SpawnPolicy, name: &str) -> Result<(), SpawnError> {
    let normalized = basename(name);
    for entry in policy.libraries {
        let matched = if cfg!(windows) {
            normalized.eq_ignore_ascii_case(entry)
        } else {
            normalized == *entry
        };
        if matched {
            return Err(SpawnError::ForbiddenSpawn {
                command: name.to_string(),
                reason: "external statistical runtime shared library",
            });
        }
    }
    Ok(())
}

/// RAII-style scope helper：构造一个 [`SpawnPolicy::forbid_external_runtimes`]
/// 实例并把它借给 `scope` 闭包。
///
/// 闭包返回 `Ok(value)` 时透传 `value`；返回 `Err` 时透传错误。
/// 闭包内部代码每次 spawn 之前都应当 `policy.check(cmd)?;` 先过闸门。
///
/// 这是 task 2.5 (`sidecar::generate_snippet`) 与 task 6.7
/// (`snapshot::export_snapshot`) 包裹各自调用栈的入口；本 task 只搭好
/// 闸门函数，不预先实现那两条流水线。
///
/// # Errors
/// 透传 `scope` 返回的 [`SpawnError`]。
pub fn forbid_external_runtimes_scope<F, T>(scope: F) -> Result<T, SpawnError>
where
    F: FnOnce(&SpawnPolicy) -> Result<T, SpawnError>,
{
    let policy = SpawnPolicy::forbid_external_runtimes();
    scope(&policy)
}

/// 取 `s` 的 basename：返回最后一个 `/` 或 `\\` 之后的子串；若没有分隔
/// 符则返回 `s` 本身。
fn basename(s: &str) -> &str {
    let mut end = s.len();
    // 去掉尾随的分隔符以避免「`/usr/bin/`」一类输入返回空字符串。
    while end > 0 {
        let last = s.as_bytes()[end - 1];
        if last == b'/' || last == b'\\' {
            end -= 1;
        } else {
            break;
        }
    }
    let trimmed = &s[..end];
    match trimmed.rfind(['/', '\\']) {
        Some(idx) => &trimmed[idx + 1..],
        None => trimmed,
    }
}

/// 把命令字符串规范化到「可以与黑名单条目逐字比对」的形态。
///
/// 流程：
/// 1. 取 basename（剥离目录段）。
/// 2. Windows 上去掉尾部 `.exe`（不区分大小写）。
fn normalize_command(s: &str) -> String {
    let mut name = basename(s).to_string();
    if cfg!(windows) && name.len() >= 4 {
        let suffix_start = name.len() - 4;
        if name[suffix_start..].eq_ignore_ascii_case(".exe") {
            name.truncate(suffix_start);
        }
    }
    name
}

/// 比较已规范化的命令名 `actual` 与黑名单条目 `forbidden`，按平台选择
/// 大小写策略（见模块级文档）。
fn commands_match(actual: &str, forbidden: &str) -> bool {
    if cfg!(windows) {
        actual.eq_ignore_ascii_case(forbidden)
    } else {
        actual == forbidden
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// 测试用的 [`Spawner`]：把每次 spawn 的命令收集到内部 vector，
    /// 让我们能断言「未触发命令时调用次数为零」。
    #[derive(Default)]
    struct RecordingSpawner {
        calls: RefCell<Vec<String>>,
    }

    impl Spawner for RecordingSpawner {
        fn spawn_command(&self, command: &str) -> Result<(), SpawnError> {
            self.calls.borrow_mut().push(command.to_string());
            Ok(())
        }
    }

    fn assert_forbidden(result: Result<(), SpawnError>, expected_command: &str) {
        match result {
            Err(SpawnError::ForbiddenSpawn { command, reason }) => {
                assert_eq!(command, expected_command, "原始命令字符串应被原样保留");
                assert!(!reason.is_empty(), "reason 应是非空静态标签");
            }
            other => panic!("expected ForbiddenSpawn for {expected_command:?}, got {other:?}"),
        }
    }

    #[test]
    fn empty_policy_allows_unrelated_commands() {
        let policy = SpawnPolicy::forbid_external_runtimes();
        assert!(policy.check("ls").is_ok());
        assert!(policy.check("git").is_ok());
        assert!(policy.check("cargo").is_ok());
        assert!(policy.check("/usr/bin/ls").is_ok());
    }

    #[test]
    fn blocklisted_bare_name_is_rejected() {
        let policy = SpawnPolicy::forbid_external_runtimes();
        assert_forbidden(policy.check("Rscript"), "Rscript");
    }

    #[test]
    fn every_canonical_blocklist_entry_is_rejected() {
        let policy = SpawnPolicy::forbid_external_runtimes();
        for entry in FORBIDDEN_COMMANDS {
            assert_forbidden(policy.check(entry), entry);
        }
    }

    #[cfg(windows)]
    #[test]
    fn blocklisted_with_exe_suffix_is_rejected_on_windows() {
        let policy = SpawnPolicy::forbid_external_runtimes();
        assert_forbidden(policy.check("Rscript.exe"), "Rscript.exe");
        // 大小写混合的 .exe 后缀也应被剥离。
        assert_forbidden(policy.check("Rscript.EXE"), "Rscript.EXE");
        assert_forbidden(policy.check("PYTHON3.EXE"), "PYTHON3.EXE");
    }

    #[cfg(unix)]
    #[test]
    fn blocklisted_with_directory_prefix_is_rejected_on_unix() {
        let policy = SpawnPolicy::forbid_external_runtimes();
        assert_forbidden(policy.check("/usr/bin/python3"), "/usr/bin/python3");
        assert_forbidden(policy.check("/usr/local/bin/Rscript"), "/usr/local/bin/Rscript");
    }

    #[cfg(windows)]
    #[test]
    fn blocklisted_with_directory_prefix_is_rejected_on_windows() {
        let policy = SpawnPolicy::forbid_external_runtimes();
        assert_forbidden(
            policy.check(r"C:\Program Files\R\bin\R.exe"),
            r"C:\Program Files\R\bin\R.exe",
        );
        assert_forbidden(
            policy.check(r"C:\Python311\python.exe"),
            r"C:\Python311\python.exe",
        );
    }

    #[test]
    fn case_sensitivity_contract_matches_platform() {
        let policy = SpawnPolicy::forbid_external_runtimes();
        if cfg!(windows) {
            // Windows: 大小写不敏感，`RSCRIPT` 等价于 `Rscript`。
            assert_forbidden(policy.check("RSCRIPT"), "RSCRIPT");
            assert_forbidden(policy.check("rscript"), "rscript");
        } else {
            // Unix: 精确大小写，只有黑名单中给出的字面量本身才匹配。
            assert!(
                policy.check("RSCRIPT").is_ok(),
                "Unix 上 `RSCRIPT` 不应等价于 `Rscript`",
            );
            assert!(
                policy.check("rscript").is_ok(),
                "Unix 上 `rscript` 不应等价于 `Rscript`",
            );
            // sanity：原本就是小写的 `python` 仍然命中。
            assert_forbidden(policy.check("python"), "python");
        }
    }

    #[test]
    fn guard_rejects_blocklisted_without_invoking_inner_spawner() {
        let inner = RecordingSpawner::default();
        let guard = SpawnPolicy::forbid_external_runtimes().wrap_spawner(inner);
        let result = guard.spawn_command("Rscript");
        assert_forbidden(result, "Rscript");
        assert!(
            guard.inner.calls.borrow().is_empty(),
            "guard 必须在内层 spawner 之前短路；实际调用 = {:?}",
            guard.inner.calls.borrow(),
        );
    }

    #[test]
    fn guard_delegates_non_blocklisted_to_inner_spawner() {
        let inner = RecordingSpawner::default();
        let guard = SpawnPolicy::forbid_external_runtimes().wrap_spawner(inner);
        guard
            .spawn_command("ls")
            .expect("non-blocklisted command should be delegated");
        assert_eq!(
            guard.inner.calls.borrow().as_slice(),
            &["ls".to_string()],
            "guard 应把非黑名单命令原样转发给内层 spawner",
        );
    }

    #[test]
    fn forbid_external_runtimes_scope_propagates_inner_value() {
        let value: u32 = forbid_external_runtimes_scope(|policy| {
            // 闭包内部可以借到策略并 ad-hoc 检查命令名。
            policy.check("ls")?;
            Ok(42)
        })
        .expect("Ok 闭包应把内部值原样透传");
        assert_eq!(value, 42);
    }

    #[test]
    fn forbid_external_runtimes_scope_propagates_forbidden_error() {
        let result: Result<(), SpawnError> = forbid_external_runtimes_scope(|policy| {
            policy.check("Rscript")?;
            Ok(())
        });
        assert_forbidden(result, "Rscript");
    }

    #[test]
    fn check_library_load_forbids_runtime_libraries() {
        let policy = SpawnPolicy::forbid_external_runtimes();
        match check_library_load(&policy, "libR.so") {
            Err(SpawnError::ForbiddenSpawn { command, reason }) => {
                assert_eq!(command, "libR.so");
                assert!(reason.contains("library"));
            }
            other => panic!("expected ForbiddenSpawn for libR.so, got {other:?}"),
        }
    }

    #[test]
    fn check_library_load_allows_unrelated_libraries() {
        let policy = SpawnPolicy::forbid_external_runtimes();
        check_library_load(&policy, "libssl.so").expect("libssl.so 不应命中黑名单");
        check_library_load(&policy, "libcrypto.so").expect("libcrypto.so 不应命中黑名单");
    }

    #[test]
    fn check_library_load_handles_directory_prefix() {
        // 库名带目录前缀也应能正确取 basename 后比对。
        let policy = SpawnPolicy::forbid_external_runtimes();
        let candidate = if cfg!(windows) {
            r"C:\Program Files\R\bin\x64\R.dll"
        } else {
            "/usr/lib/R/lib/libR.so"
        };
        match check_library_load(&policy, candidate) {
            Err(SpawnError::ForbiddenSpawn { command, .. }) => {
                assert_eq!(command, candidate);
            }
            other => panic!("expected ForbiddenSpawn for {candidate:?}, got {other:?}"),
        }
    }

    // -- 内部 helper 覆盖 --------------------------------------------------

    #[test]
    fn basename_strips_directory_segments() {
        assert_eq!(basename("Rscript"), "Rscript");
        assert_eq!(basename("/usr/bin/Rscript"), "Rscript");
        assert_eq!(basename(r"C:\bin\Rscript.exe"), "Rscript.exe");
        assert_eq!(basename("./local/Rscript"), "Rscript");
        // 尾随分隔符不应让 basename 返回空串。
        assert_eq!(basename("/usr/bin/"), "bin");
    }

    #[test]
    fn normalize_command_drops_exe_suffix_only_on_windows() {
        if cfg!(windows) {
            assert_eq!(normalize_command("Rscript.exe"), "Rscript");
            assert_eq!(normalize_command("Rscript.EXE"), "Rscript");
            assert_eq!(normalize_command(r"C:\bin\Rscript.exe"), "Rscript");
        } else {
            // Unix 上 `Rscript.exe` 是一个独立文件名，不应被剥离。
            assert_eq!(normalize_command("Rscript.exe"), "Rscript.exe");
        }
        // basename 行为在两个平台都一致。
        assert_eq!(normalize_command("/usr/bin/python3"), "python3");
    }
}
