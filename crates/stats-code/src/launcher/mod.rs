//! Stats Code Launcher 模块树。
//!
//! 此模块在 `stats-code.exe` 无子命令调用时提供启动器逻辑：端口扫描、
//! Agent_Backend 启动、前端伺服 / Vite 子进程、Lock_File 单实例、按需打开
//! 浏览器以及 LLM 配置存取。
//!
//! 详见 `.kiro/specs/single-command-launcher/design.md`。

pub mod args;
pub mod backend;
pub mod browser;
pub mod config_store;
pub mod frontend;
pub mod lock;
pub mod paths;
pub mod port;
pub mod process_guard;
pub mod providers;

use std::io::{self, Write};
use std::sync::Arc;

use crate::launcher::args::LauncherArgs;
use crate::launcher::backend::RunMode;
use crate::launcher::frontend::FrontendError;
use crate::launcher::lock::{AcquireOutcome, LockError, LockFileV1};
use crate::launcher::port::ScanError;

// ---------------------------------------------------------------------------
// LauncherError — 结构化错误与退出码映射 (task 15.1)
// ---------------------------------------------------------------------------

/// Launcher 主流程可能产生的错误，每个变体对应一个明确的进程退出码。
///
/// 退出码分配（与 design.md Error Handling 表对齐）：
/// - 2: `AllPortsBusy`
/// - 3: `LockIo`
/// - 4: `ViteSpawnFailed`
/// - 5: `ViteExitedEarly`
/// - 1: Other（兜底）
#[derive(Debug)]
pub enum LauncherError {
    /// 端口扫描区间内所有端口均不可用 → exit 2。
    AllPortsBusy(String),
    /// `Lock_File` I/O 错误 → exit 3。
    LockIo(String),
    /// Vite 子进程启动失败（dev 模式）→ exit 4。
    ViteSpawnFailed(String),
    /// Vite 子进程在主进程仍存活时退出（dev 模式）→ exit 5。
    ViteExitedEarly(String),
    /// 其他非预期错误 → exit 1。
    Other(String),
}

impl LauncherError {
    /// 返回该错误对应的进程退出码。
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::AllPortsBusy(_) => 2,
            Self::LockIo(_) => 3,
            Self::ViteSpawnFailed(_) => 4,
            Self::ViteExitedEarly(_) => 5,
            Self::Other(_) => 1,
        }
    }
}

impl std::fmt::Display for LauncherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AllPortsBusy(msg) => write!(f, "{msg}"),
            Self::LockIo(msg) => write!(f, "{msg}"),
            Self::ViteSpawnFailed(msg) => write!(f, "{msg}"),
            Self::ViteExitedEarly(msg) => write!(f, "{msg}"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for LauncherError {}

impl From<ScanError> for LauncherError {
    fn from(e: ScanError) -> Self {
        match e {
            ScanError::AllPortsBusy { tried, last_error } => Self::AllPortsBusy(format!(
                "所有端口 {}..{} 均被占用 (last OS error: {last_error})",
                tried.start, tried.end_exclusive
            )),
        }
    }
}

impl From<LockError> for LauncherError {
    fn from(e: LockError) -> Self {
        Self::LockIo(e.to_string())
    }
}

impl From<FrontendError> for LauncherError {
    fn from(e: FrontendError) -> Self {
        match e {
            FrontendError::ViteSpawnFailed(msg) => Self::ViteSpawnFailed(msg),
            FrontendError::ViteExitedEarly => {
                Self::ViteExitedEarly("Vite 子进程在主进程仍存活时异常退出".to_string())
            }
            FrontendError::EmbeddedAssetMissing => {
                Self::Other("内嵌前端资源中缺少 index.html".to_string())
            }
        }
    }
}

impl From<io::Error> for LauncherError {
    fn from(e: io::Error) -> Self {
        Self::Other(e.to_string())
    }
}

/// Launcher 主流程编排器。
///
/// 持有经 CLI 解析出的 [`LauncherArgs`]（当前仅 `--no-browser`）。调用
/// [`Launcher::run`] 执行完整启动流程。
#[derive(Debug)]
pub struct Launcher {
    pub args: LauncherArgs,
}

impl Default for Launcher {
    fn default() -> Self {
        Self {
            args: LauncherArgs { no_browser: false },
        }
    }
}

impl Launcher {
    /// 创建一个带参数的 Launcher 实例。
    #[must_use]
    pub fn new(args: LauncherArgs) -> Self {
        Self { args }
    }

    /// 启动 launcher 主流程。
    ///
    /// `顺序：try_acquire` → `scan_first_bindable` → 起 backend → `ensure_frontend`
    /// → `write_running` → open browser → 阻塞等 Ctrl+C。
    ///
    /// 已有实例分支：直接 open(existing.url) 后退出 0。
    ///
    /// # Errors
    /// 返回 [`LauncherError`]，调用方可通过 [`LauncherError::exit_code`] 获取退出码。
    pub fn run(&self) -> Result<(), LauncherError> {
        let mut stdout = io::stdout();

        // 1. 确保 app data 目录存在并解析 Lock_File 路径
        let _app_dir =
            paths::ensure_app_data_dir().map_err(|e| LauncherError::Other(e.to_string()))?;
        let lock_path = paths::lock_file_path().map_err(|e| LauncherError::Other(e.to_string()))?;

        // 2. try_acquire: 注入真实的 pid/port 探测
        let outcome = lock::try_acquire(
            &lock_path,
            std::process::id(),
            is_pid_alive,
            is_port_reachable,
        )?;

        match outcome {
            AcquireOutcome::Existing { url, pid: _ } => {
                // 已有存活实例：打开其 URL 后退出 0
                browser::open(&url, self.args.no_browser, &mut stdout)?;
                return Ok(());
            }
            AcquireOutcome::Acquired(mut handle) => {
                // 3. 端口扫描
                let listener = port::scan_first_bindable(port::DEFAULT_RANGE)?;
                let actual_port = listener.local_addr()?.port();

                // 4. 确定 RunMode
                let run_mode = if cfg!(feature = "dev-vite") {
                    RunMode::Dev
                } else {
                    RunMode::Prod
                };

                // 5. 起 backend（axum server in async runtime）
                let rt = tokio::runtime::Runtime::new()?;
                let std_listener = listener;

                // 构造 AppState
                let session_store: Arc<dyn agent_core::traits::session_store::SessionStore> =
                    Arc::new(agent_core::store::MemSessionStore::new());
                let dataset_store: Arc<dyn agent_core::traits::dataset_store::DatasetStore> =
                    Arc::new(
                        rt.block_on(agent_core::store::FsDatasetStore::new(
                            _app_dir.join("datasets"),
                        ))
                        .map_err(|e| LauncherError::Other(e.to_string()))?,
                    );
                let mut app_state = agent_server::state::AppState::new(session_store.clone());
                app_state.dataset_store = Some(dataset_store.clone());

                // 注入 LlmConfigStore — 通过 adapter 桥接本地 trait 与 agent-server trait
                let config_path =
                    paths::config_file_path().map_err(|e| LauncherError::Other(e.to_string()))?;
                let config_store = Arc::new(LlmConfigStoreAdapter(
                    config_store::TomlFileStore::new(config_path),
                ));
                app_state.llm_config_store = Some(config_store.clone());
                app_state.llm_probe = Some(Arc::new(HttpLlmProbe));

                // Construct the Run-State Store (shared between the orchestrator
                // and the Run-Backed Snapshot Provider) — Requirement 6.1.
                let run_store: Arc<dyn agent_core::traits::run_store::RunStore> =
                    Arc::new(agent_core::store::MemRunStore::new());

                // 注入动态构建的 message_handler
                let run_env = build_run_environment();
                let dynamic_handler = Arc::new(LlmConfigurableMessageHandler::new(
                    config_store.clone(),
                    session_store,
                    dataset_store.clone(),
                    run_env,
                    Some(run_store.clone()),
                ));
                app_state.message_handler = Some(dynamic_handler);

                // 注入 Parity & Multi-Lang Sidecar 三个 provider。
                // coverage-matrix 与 sidecar 都完整可用：coverage-matrix 的
                // 数据源是编译期内嵌矩阵；sidecar 是纯函数，列元数据 /
                // 数据集 SHA256 / 参数全部由前端在请求体里带来，无需 run-state。
                // snapshot 导出由 RunBackedSnapshotProvider 支持，它从
                // MemRunStore 读取已记录的 Analysis Run 并委托给 export_snapshot。
                app_state.coverage_matrix_provider =
                    Some(Arc::new(providers::EmbeddedCoverageMatrixProvider));
                app_state.sidecar_provider = Some(Arc::new(providers::LiveSidecarProvider));

                // Construct the Run-Backed Snapshot Provider (Requirement 6.1).
                // The provider reads dataset bytes through the DatasetStore
                // trait (which owns its on-disk layout), so we hand it the same
                // shared dataset store the message handler uses.
                let api_keys_for_redaction: Vec<String> = {
                    use agent_server::handlers::llm_config::LlmConfigStore;
                    config_store
                        .read()
                        .filter(|cfg| !cfg.api_key.is_empty())
                        .map(|cfg| vec![cfg.api_key])
                        .unwrap_or_default()
                };
                app_state.snapshot_provider = Some(Arc::new(
                    providers::RunBackedSnapshotProvider::new(
                        run_store.clone(),
                        dataset_store,
                        api_keys_for_redaction,
                        None, // working_directory: not configured yet
                    ),
                ));

                let load_counter = agent_server::middleware::load_shedding::LoadCounter::new(50);
                let app = agent_server::build_router(load_counter, app_state);

                // 把 std TcpListener 转换为 tokio 的
                std_listener.set_nonblocking(true)?;
                let tokio_listener =
                    rt.block_on(async { tokio::net::TcpListener::from_std(std_listener) })?;

                // spawn axum server task
                rt.spawn(async move {
                    if let Err(e) = axum::serve(tokio_listener, app).await {
                        eprintln!("agent-server error: {e}");
                    }
                });

                // 6. ensure_frontend
                let public_url;
                // Keep _frontend_guard alive to maintain Job Object → Vite stays running.
                let _frontend_guard;
                match run_mode {
                    RunMode::Prod => {
                        public_url = format!("http://127.0.0.1:{actual_port}/");
                        _frontend_guard = None;
                    }
                    RunMode::Dev => {
                        match frontend::ensure_frontend(&format!("http://127.0.0.1:{actual_port}/"))
                        {
                            Ok(frontend::FrontendHandle::DevVite(guard, url)) => {
                                public_url = url;
                                _frontend_guard = Some(guard);
                            }
                            Ok(frontend::FrontendHandle::EmbeddedProd) => {
                                public_url = format!("http://127.0.0.1:{actual_port}/");
                                _frontend_guard = None;
                            }
                            Err(e) => return Err(LauncherError::from(e)),
                        }
                    }
                }

                // 7. write_running
                let record = LockFileV1::new(
                    std::process::id(),
                    &public_url,
                    chrono_now_iso(),
                    if run_mode == RunMode::Dev {
                        "dev"
                    } else {
                        "prod"
                    },
                );
                handle.write_running(&record)?;

                // 8. 输出启动日志
                let ready_line = backend::format_ready_line(actual_port, run_mode);
                writeln!(stdout, "{ready_line}")?;

                // 9. open browser
                browser::open(&public_url, self.args.no_browser, &mut stdout)?;

                // 10. 阻塞等 Ctrl+C（dev 模式下同时监听 Vite 早退）
                if let Some(ref guard) = _frontend_guard {
                    // Dev 模式：spawn 一个线程监听 Vite 子进程退出
                    let (vite_exit_tx, vite_exit_rx) = std::sync::mpsc::channel::<()>();
                    // 注意：_frontend_guard 持有 ProcessGuard，但 wait_child 需要
                    // &mut self。由于 ProcessGuard 的 child stdout/stderr 已被 take，
                    // child.wait() 仅等待 OS handle。这里通过子线程监听 child pid 退出。
                    let vite_pid = guard.child_pid();
                    std::thread::Builder::new()
                        .name("vite-exit-monitor".into())
                        .spawn(move || {
                            // 通过轮询检查 pid 是否还活着
                            loop {
                                std::thread::sleep(std::time::Duration::from_secs(1));
                                if !is_pid_alive(vite_pid) {
                                    let _ = vite_exit_tx.send(());
                                    break;
                                }
                            }
                        })
                        .ok();

                    rt.block_on(wait_for_dev_shutdown(vite_exit_rx))?;
                } else {
                    // Prod 模式：仅等 Ctrl+C
                    rt.block_on(async {
                        tokio::signal::ctrl_c().await.ok();
                    });
                }

                writeln!(stdout, "\nshutting down...")?;

                // handle drops here → lock file deleted
            }
        }

        Ok(())
    }
}

/// 判断给定 PID 的进程是否仍然存活。
#[cfg(windows)]
#[allow(unsafe_code)]
fn is_pid_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    // SAFETY: OpenProcess with a valid flags value and pid is safe; returns NULL on failure.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }
    // SAFETY: handle is valid and non-null.
    unsafe { CloseHandle(handle) };
    true
}

#[cfg(not(windows))]
fn is_pid_alive(pid: u32) -> bool {
    // Check /proc/<pid> on Linux; fallback to assuming alive on other Unix
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// 探测 URL 对应的端口是否可达（TCP connect）。
fn is_port_reachable(url: &str) -> bool {
    // 从 URL 中提取 host:port — "http://127.0.0.1:8080/" → "127.0.0.1:8080"
    let url = url.trim_end_matches('/');
    let addr_str = url.strip_prefix("http://").unwrap_or(url);

    let addr: std::net::SocketAddr = match addr_str.parse() {
        Ok(a) => a,
        Err(_) => return false, // URL 格式异常 → 视为不可达
    };

    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(500)).is_ok()
}

/// 返回当前时间的 ISO-8601 字符串（UTC，不引入 chrono）。
///
/// 格式：`2025-06-01T12:34:56Z`。用 `time` 标准库手算年月日时分秒，
/// 精度到秒，足够诊断用途。
fn chrono_now_iso() -> String {
    use std::time::SystemTime;

    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|e| e.duration())
        .as_secs();

    // 简单的 unix timestamp → UTC 分解（不处理闰秒，误差可接受）
    let days = secs / 86400;
    let day_secs = secs % 86400;
    let h = day_secs / 3600;
    let m = (day_secs % 3600) / 60;
    let s = day_secs % 60;

    // 从 1970-01-01 起算天数 → 年月日
    let (year, month, day) = days_to_ymd(days);

    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// 把从 1970-01-01 起的天数转换为 (year, month, day)。
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // 算法来自 Howard Hinnant's `civil_from_days`
    let days = days as i64 + 719_468; // shift epoch to 0000-03-01
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let doe = (days - era * 146_097) as u64; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u64, m, d)
}

// ---------------------------------------------------------------------------
// LlmConfigStore adapter: bridges stats-code's local store to agent-server's trait
// ---------------------------------------------------------------------------

/// Adapter that implements `agent_server::handlers::llm_config::LlmConfigStore`
/// by delegating to a `config_store::TomlFileStore`.
struct LlmConfigStoreAdapter(config_store::TomlFileStore);

impl agent_server::handlers::llm_config::LlmConfigStore for LlmConfigStoreAdapter {
    fn read(&self) -> Option<agent_server::handlers::llm_config::LlmConfig> {
        use crate::launcher::config_store::LlmConfigStore as LocalStore;

        LocalStore::read(&self.0).ok()?
    }

    fn write(&self, config: &agent_server::handlers::llm_config::LlmConfig) -> Result<(), String> {
        use crate::launcher::config_store::LlmConfigStore as LocalStore;

        LocalStore::write(&self.0, config).map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// HttpLlmProbe: 真实连通性探测（向 LLM API 发轻量请求确认 key 有效）
// ---------------------------------------------------------------------------

/// 真实 LLM 连通性探测：向 DeepSeek/OpenAI 的 models endpoint 发 GET 请求。
///
/// 不发送聊天请求（省 token），只验证 API Key 能通过认证。
struct HttpLlmProbe;

#[async_trait::async_trait]
impl agent_server::handlers::llm_config::LlmProbe for HttpLlmProbe {
    async fn probe(
        &self,
        provider: agent_server::handlers::llm_config::LlmProvider,
        api_key: &str,
        base_url: Option<&str>,
        _model: Option<&str>,
    ) -> Result<(), String> {
        use agent_server::handlers::llm_config::LlmProvider;

        let default_base = match provider {
            LlmProvider::DeepSeek => "https://api.deepseek.com/v1",
            LlmProvider::OpenAi => "https://api.openai.com/v1",
        };

        let base = base_url.unwrap_or(default_base).trim();
        let base_trimmed = base.trim_end_matches('/');

        let url = if base_trimmed.ends_with("/models") {
            base_trimmed.to_string()
        } else {
            format!("{base_trimmed}/models")
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| format!("HTTP client 创建失败: {e}"))?;

        let resp = client
            .get(url)
            .header("Authorization", format!("Bearer {api_key}"))
            .send()
            .await
            .map_err(|e| format!("连接 {provider:?} API 失败: {e}"))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(format!("{provider:?} API 返回 {status}: {body}",))
        }
    }
}

/// `argv` 分流后的运行模式。
///
/// 与 `cli::Command` 的所有变体对应的子命令名是该枚举判定的核心输入，
/// 由 `KNOWN_SUBCOMMANDS` 常量集中维护。`main.rs` 在 task 8.2 中按此枚举
/// 决定是走 Launcher 路径还是回到既有的 `lib::run()` 子命令分发。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// 无子命令调用：进入 Stats Code Launcher 模式（Requirement 1.1）。
    Launcher,
    /// 命中已知子命令：走既有 `lib::run()` 路径，供
    /// `SkillInvoker::StatsCli` 等内部调用（Requirement 2.5）。
    Subcommand,
}

/// 已知的顶层子命令名集合，对应 `cli::Command` 的全部变体。
///
/// clap 默认将枚举变体名渲染为小写形式作为 CLI 子命令名，因此此处保持
/// 小写字符串字面量。任何对 `cli::Command` 的增删都需要同步更新此常量。
pub const KNOWN_SUBCOMMANDS: &[&str] = &[
    "config",
    "init",
    "doctor",
    "plan",
    "check",
    "inspect",
    "tableone",
    "rate",
    "power",
    "diagnostic",
    "survival",
    "auth",
    "ai",
    "audit",
    "model",
    "report",
    "open",
    "workflow",
    "run",
    "stats",
    // Feature: parity-and-multilang-sidecar, task 9.1.
    // The `parity` Internal Subcommand is hidden from `--help`
    // (Requirement 5.2) but must still route to the subcommand path so it
    // bypasses `Launcher::run` entirely (Requirement 5.8).
    "parity",
    // Feature: parity-and-multilang-sidecar, task 7.1.
    // The `replay` Internal Subcommand is hidden from `--help`
    // (Requirement 8.3) but must still route to the subcommand path so it
    // bypasses `Launcher::run` entirely (no port bind, no browser launch,
    // no instance lock).
    "replay",
];

/// 根据进程 argv 决定走 Launcher 路径还是子命令分发路径。
///
/// 约定：argv\[0\] 为程序名本身，从 argv\[1\] 起为用户参数；以 `-` 开头的
/// 词元被视作旗标（含其值，因为 launcher 自身的所有公开旗标均不带独立值）
/// 直接跳过；任意非旗标词元若命中 `KNOWN_SUBCOMMANDS` 即视为子命令调用。
///
/// 这对应设计文档 Property 1：
/// - 若 argv 中不出现任何已知子命令名（且不出现 `--help` / `--version`），
///   返回 `Mode::Launcher`；
/// - 若 argv 中至少出现一个已知子命令名，返回 `Mode::Subcommand`。
///
/// Requirements: 1.1, 2.4, 2.5
#[must_use]
pub fn classify_invocation(argv: &[String]) -> Mode {
    for arg in argv.iter().skip(1) {
        if arg.starts_with('-') {
            continue;
        }
        if KNOWN_SUBCOMMANDS.contains(&arg.as_str()) {
            return Mode::Subcommand;
        }
    }
    Mode::Launcher
}

async fn wait_for_dev_shutdown(
    vite_exit_rx: std::sync::mpsc::Receiver<()>,
) -> Result<(), LauncherError> {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => Ok(()),
        received = tokio::task::spawn_blocking(move || vite_exit_rx.recv()) => {
            match received {
                Ok(Ok(())) => Err(LauncherError::ViteExitedEarly(
                    "Vite child process exited while launcher was still running".to_string(),
                )),
                Ok(Err(_)) => Ok(()),
                Err(e) => Err(LauncherError::Other(format!("Vite monitor task failed: {e}"))),
            }
        }
    }
}

#[cfg(test)]
mod launcher_service_tests {
    use super::*;

    #[test]
    fn wait_for_dev_shutdown_returns_exit_five_when_vite_exits() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(()).expect("send vite exit signal");

        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let err = rt
            .block_on(wait_for_dev_shutdown(rx))
            .expect_err("vite exit should be reported as launcher error");

        assert_eq!(err.exit_code(), 5);
    }

    #[tokio::test]
    async fn llm_handler_keeps_shared_session_and_dataset_stores() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config_store = Arc::new(LlmConfigStoreAdapter(config_store::TomlFileStore::new(
            tmp.path().join("config.toml"),
        )));
        let session_store: Arc<dyn agent_core::traits::session_store::SessionStore> =
            Arc::new(agent_core::store::MemSessionStore::new());
        let dataset_store: Arc<dyn agent_core::traits::dataset_store::DatasetStore> = Arc::new(
            agent_core::store::FsDatasetStore::new(tmp.path().join("datasets"))
                .await
                .expect("dataset store"),
        );

        let handler = LlmConfigurableMessageHandler::new(
            config_store,
            session_store.clone(),
            dataset_store.clone(),
            build_run_environment(),
            None,
        );

        assert!(Arc::ptr_eq(&handler.session_store, &session_store));
        assert!(Arc::ptr_eq(&handler.dataset_store, &dataset_store));
    }
}

#[cfg(test)]
mod invocation_tests {
    use super::*;
    use proptest::prelude::*;

    fn safe_arg() -> impl Strategy<Value = String> {
        prop_oneof![
            "[a-z][a-z0-9_-]{0,12}"
                .prop_filter("generated token must not be a known subcommand", |arg| {
                    !KNOWN_SUBCOMMANDS.contains(&arg.as_str())
                },),
            Just("--no-browser".to_string()),
            Just("--arbitrary-flag".to_string()),
        ]
    }

    fn known_subcommand() -> impl Strategy<Value = &'static str> {
        prop::sample::select(KNOWN_SUBCOMMANDS.to_vec())
    }

    proptest! {
        // Feature: single-command-launcher, Property 1:
        //   argv without a known subcommand routes to launcher mode.
        #[test]
        fn classify_invocation_routes_launcher_without_known_subcommands(
            args in proptest::collection::vec(safe_arg(), 0..32)
        ) {
            let mut argv = vec!["stats-code".to_string()];
            argv.extend(args);

            prop_assert_eq!(classify_invocation(&argv), Mode::Launcher);
        }

        // Feature: single-command-launcher, Property 1:
        //   argv containing any known subcommand routes to subcommand mode.
        #[test]
        fn classify_invocation_routes_subcommand_when_known_name_is_present(
            prefix in proptest::collection::vec(safe_arg(), 0..16),
            subcommand in known_subcommand(),
            suffix in proptest::collection::vec(safe_arg(), 0..16)
        ) {
            let mut argv = vec!["stats-code".to_string()];
            argv.extend(prefix);
            argv.push(subcommand.to_string());
            argv.extend(suffix);

            prop_assert_eq!(classify_invocation(&argv), Mode::Subcommand);
        }
    }

    // Feature: parity-and-multilang-sidecar, task 15.2.
    // Validates: Requirements 5.3, 5.8, 8.3, 10.3 — when argv carries the
    // hidden `parity` Internal Subcommand, `classify_invocation` must
    // return `Mode::Subcommand` so `main.rs` routes through `lib::run`
    // (and ultimately `parity::run_local`) instead of `Launcher::run`.
    // This is the "explicit not-launcher" branch the spec calls out:
    // no `try_acquire(LockFile)`, no `scan_first_bindable`, no
    // `open(url)` are reachable from `Mode::Subcommand`.
    #[test]
    fn classify_invocation_routes_parity_to_subcommand_path() {
        let argv = vec!["stats-code".to_string(), "parity".to_string()];
        assert_eq!(classify_invocation(&argv), Mode::Subcommand);
    }

    // Feature: parity-and-multilang-sidecar, task 15.2.
    // Validates: Requirements 5.3, 5.8 — `--filter <id>` after the
    // `parity` token must not flip the dispatch back to the launcher
    // path. `parity --filter tableone` is still a subcommand invocation
    // and therefore bypasses `Launcher::run`.
    #[test]
    fn classify_invocation_routes_parity_with_filter_flag_to_subcommand_path() {
        let argv = vec![
            "stats-code".to_string(),
            "parity".to_string(),
            "--filter".to_string(),
            "tableone".to_string(),
        ];
        assert_eq!(classify_invocation(&argv), Mode::Subcommand);
    }

    // Feature: parity-and-multilang-sidecar, task 15.2.
    // Validates: Requirements 8.3, 10.3 — when argv carries the hidden
    // `replay` Internal Subcommand, `classify_invocation` must return
    // `Mode::Subcommand`. Same launcher-bypass guarantee as parity:
    // `replay <SNAPSHOT>` never reaches `Launcher::run`.
    #[test]
    fn classify_invocation_routes_replay_to_subcommand_path() {
        let argv = vec![
            "stats-code".to_string(),
            "replay".to_string(),
            "snapshot.zip".to_string(),
        ];
        assert_eq!(classify_invocation(&argv), Mode::Subcommand);
    }

    // Feature: parity-and-multilang-sidecar, task 15.2.
    // Validates: Requirement 10.3 — empty argv (the bare
    // `stats-code` invocation) must still route to the launcher,
    // confirming task 15.2's wiring did not regress the no-arg path.
    #[test]
    fn classify_invocation_routes_empty_argv_to_launcher() {
        let argv = vec!["stats-code".to_string()];
        assert_eq!(classify_invocation(&argv), Mode::Launcher);
    }
}

// ---------------------------------------------------------------------------
// LlmConfigurableMessageHandler: 动态构建并缓存 MessageHandler 的适配器
// ---------------------------------------------------------------------------

/// Build the `RunEnvironment` once at launcher startup (Requirement 5.8, task 8.2).
///
/// - `os_family`: maps `std::env::consts::OS` to "Windows" / "Linux" / "macOS"
///   (privacy-safe token — never the host name or user name).
/// - `os_version`: best-effort from OS APIs; `None` if unavailable.
/// - `release_version`: from `crate::RELEASE_VERSION`.
/// - `commit_sha`: from `crate::COMMIT_SHA`.
/// - `reference_software`: empty vec (populated later by the runtime per-run).
fn build_run_environment() -> agent_core::models::RunEnvironment {
    let os_family = match std::env::consts::OS {
        "windows" => "Windows".to_string(),
        "linux" => "Linux".to_string(),
        "macos" => "macOS".to_string(),
        other => other.to_string(),
    };

    let os_version = best_effort_os_version();

    agent_core::models::RunEnvironment {
        os_family,
        os_version,
        release_version: crate::RELEASE_VERSION.to_string(),
        commit_sha: crate::COMMIT_SHA.to_string(),
        reference_software: Vec::new(),
    }
}

/// Best-effort OS version detection.
///
/// On Windows: reads the `ProductName` + `CurrentBuildNumber` from the registry
/// via environment variables or falls back to `None`.
/// On other platforms: returns `None` (can be extended later with `/etc/os-release`
/// parsing on Linux or `sw_vers` on macOS).
fn best_effort_os_version() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        // Try reading from the Windows registry via `ver` command output or
        // environment variables. The simplest portable approach is to read
        // the OS_VERSION env var set by some CI systems, or fall back to the
        // Windows build number from the `PROCESSOR_ARCHITECTURE` family.
        // For a lightweight approach, use the `winapi`-free method:
        // `std::env::var("OS")` returns "Windows_NT" on all modern Windows.
        // We combine with the build number from `cmd /c ver` if available.
        use std::process::Command;
        let output = Command::new("cmd")
            .args(["/c", "ver"])
            .output()
            .ok()?;
        if output.status.success() {
            let ver = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !ver.is_empty() {
                return Some(ver);
            }
        }
        None
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

struct LlmConfigurableMessageHandler {
    config_store: Arc<LlmConfigStoreAdapter>,
    session_store: Arc<dyn agent_core::traits::session_store::SessionStore>,
    dataset_store: Arc<dyn agent_core::traits::dataset_store::DatasetStore>,
    run_environment: agent_core::models::RunEnvironment,
    run_store: Option<Arc<dyn agent_core::traits::run_store::RunStore>>,
    cached_handler: Arc<
        tokio::sync::RwLock<
            Option<(
                agent_server::handlers::llm_config::LlmConfig,
                Arc<dyn agent_server::state::MessageHandler>,
            )>,
        >,
    >,
}

impl LlmConfigurableMessageHandler {
    fn new(
        config_store: Arc<LlmConfigStoreAdapter>,
        session_store: Arc<dyn agent_core::traits::session_store::SessionStore>,
        dataset_store: Arc<dyn agent_core::traits::dataset_store::DatasetStore>,
        run_environment: agent_core::models::RunEnvironment,
        run_store: Option<Arc<dyn agent_core::traits::run_store::RunStore>>,
    ) -> Self {
        Self {
            config_store,
            session_store,
            dataset_store,
            run_environment,
            run_store,
            cached_handler: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }
}

impl agent_server::state::MessageHandler for LlmConfigurableMessageHandler {
    fn handle_message(
        &self,
        sid: agent_core::models::SessionId,
        msg: agent_core::orchestrator::UserMessageInput,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = std::pin::Pin<
                        Box<
                            dyn tokio_stream::Stream<Item = agent_core::orchestrator::AgentEvent>
                                + Send,
                        >,
                    >,
                > + Send
                + '_,
        >,
    > {
        use agent_server::handlers::llm_config::LlmConfigStore;

        let config_store = self.config_store.clone();
        let session_store = self.session_store.clone();
        let dataset_store = self.dataset_store.clone();
        let cached_handler = self.cached_handler.clone();
        let run_environment = self.run_environment.clone();
        let run_store = self.run_store.clone();

        Box::pin(async move {
            let current_config = config_store.read();

            match current_config {
                None => {
                    let error_payload = agent_core::models::ErrorPayload::new(
                        agent_core::models::ErrorCode::LlmUnavailable,
                        "AI 服务尚未初始化：未在 LLM 设置中配置 API Key。".to_string(),
                    );
                    let events = vec![
                        agent_core::orchestrator::AgentEvent::Error(error_payload),
                        agent_core::orchestrator::AgentEvent::Done,
                    ];
                    Box::pin(tokio_stream::iter(events))
                        as std::pin::Pin<
                            Box<
                                dyn tokio_stream::Stream<
                                        Item = agent_core::orchestrator::AgentEvent,
                                    > + Send,
                            >,
                        >
                }
                Some(config) => {
                    if config.api_key.trim().is_empty() {
                        let error_payload = agent_core::models::ErrorPayload::new(
                            agent_core::models::ErrorCode::LlmUnavailable,
                            "AI 服务尚未初始化：API Key 为空，请在设置中输入有效的 API Key。"
                                .to_string(),
                        );
                        let events = vec![
                            agent_core::orchestrator::AgentEvent::Error(error_payload),
                            agent_core::orchestrator::AgentEvent::Done,
                        ];
                        return Box::pin(tokio_stream::iter(events))
                            as std::pin::Pin<
                                Box<
                                    dyn tokio_stream::Stream<
                                            Item = agent_core::orchestrator::AgentEvent,
                                        > + Send,
                                >,
                            >;
                    }

                    let mut lock = cached_handler.write().await;
                    let use_cached = if let Some((cached_cfg, _)) = &*lock {
                        cached_cfg == &config
                    } else {
                        false
                    };

                    let handler = if use_cached {
                        lock.as_ref().unwrap().1.clone()
                    } else {
                        let runner = agent_core::skill::SkillRunner::new(
                            std::env::current_exe()
                                .unwrap_or_else(|_| std::path::PathBuf::from("stats-code")),
                            std::env::temp_dir(),
                            60,
                            1024,
                        );
                        let registry = agent_core::skill::SkillRegistry::with_defaults();

                        let core_llm_config = match config.provider {
                            agent_server::handlers::llm_config::LlmProvider::DeepSeek => {
                                let mut ds_cfg = agent_core::llm::DeepSeekConfig::new(
                                    secrecy::SecretString::from(config.api_key.clone()),
                                );
                                if let Some(ref url) = config.base_url {
                                    if !url.trim().is_empty() {
                                        ds_cfg.base_url = url.trim().to_string();
                                    }
                                }
                                if let Some(ref model) = config.model {
                                    if !model.trim().is_empty() {
                                        ds_cfg.model = model.trim().to_string();
                                    }
                                }
                                agent_core::llm::LlmConfig::DeepSeek(ds_cfg)
                            }
                            agent_server::handlers::llm_config::LlmProvider::OpenAi => {
                                let mut oa_cfg = agent_core::llm::OpenAiConfig::new(
                                    secrecy::SecretString::from(config.api_key.clone()),
                                );
                                if let Some(ref url) = config.base_url {
                                    if !url.trim().is_empty() {
                                        oa_cfg.base_url = url.trim().to_string();
                                    }
                                }
                                if let Some(ref model) = config.model {
                                    if !model.trim().is_empty() {
                                        oa_cfg = oa_cfg.with_model(model.trim());
                                    }
                                }
                                agent_core::llm::LlmConfig::OpenAi(oa_cfg)
                            }
                        };

                        let llm = match agent_core::llm::build_llm_provider(&core_llm_config) {
                            Ok(p) => p,
                            Err(e) => {
                                let error_payload = agent_core::models::ErrorPayload::new(
                                    agent_core::models::ErrorCode::LlmUnavailable,
                                    format!("LLM 配置初始化失败: {e}"),
                                );
                                let events = vec![
                                    agent_core::orchestrator::AgentEvent::Error(error_payload),
                                    agent_core::orchestrator::AgentEvent::Done,
                                ];
                                return Box::pin(tokio_stream::iter(events))
                                    as std::pin::Pin<
                                        Box<
                                            dyn tokio_stream::Stream<
                                                    Item = agent_core::orchestrator::AgentEvent,
                                                > + Send,
                                        >,
                                    >;
                            }
                        };

                        let orch = agent_core::orchestrator::AgentOrchestrator::new(
                            session_store,
                            dataset_store,
                            registry,
                            runner,
                            llm,
                        )
                        .with_run_environment(run_environment.clone());
                        // Wire the run store so the orchestrator records analysis
                        // runs that the RunBackedSnapshotProvider can later export.
                        let orch = if let Some(ref rs) = run_store {
                            orch.with_run_store(rs.clone())
                        } else {
                            orch
                        };
                        let adapter = Arc::new(
                            agent_server::orchestrator_adapter::OrchestratorAdapter::new(orch),
                        )
                            as Arc<dyn agent_server::state::MessageHandler>;
                        *lock = Some((config.clone(), adapter.clone()));
                        adapter
                    };

                    handler.handle_message(sid, msg).await
                }
            }
        })
    }
}
