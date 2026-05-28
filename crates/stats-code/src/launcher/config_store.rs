//! LLM 配置存储：`%APPDATA%\stats-code\config.toml` 的明文 TOML 读写
//! （Requirements 9.1, 9.2, 9.3, 9.5）。
//!
//! 设计要点（详见 `design.md` Data Models / Components and Interfaces）：
//! - **明文 TOML**：刻意不调用任何 OS 凭据加密 / 凭据保险柜机制
//!   （R9.3），用户可用记事本管理 Key；权限完全依赖 NTFS 默认继承（R9.4）。
//! - **缺失即未配置**：文件不存在或 `api_key` 为空字符串都视同未配置
//!   （R9.5、R10.2），`read()` 返回 `Ok(None)`，不阻塞 launcher 启动。
//! - **损坏即备份**：TOML 解析失败时把原文件 rename 为
//!   `config.toml.bak.<unix_ns>` 并返回 `Ok(None)`，保留排查现场又不让坏内容
//!   持续阻塞后续启动（design.md Error Handling 节 `LlmConfigError::TomlParse`）。
//!   时间戳用 `std::time::SystemTime` 取 Unix epoch **纳秒**——比秒粒度细，
//!   能区分同一秒内连续两次损坏-备份；同时不引入 `chrono` 等额外依赖。
//! - **`provider` 序列化为小写**：与前端 `LlmProvider` 类型
//!   (`'deepseek' | 'openai'`) 以及 `agent_core::models::LlmProvider`
//!   共享契约保持一致（R10.3 间接约束）。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub use agent_core::models::llm_config::{LlmConfig, LlmProvider as Provider};

/// `LlmConfigStore` 操作错误。
#[derive(Debug)]
pub enum ConfigError {
    /// 与 `%APPDATA%` 文件系统交互失败。
    Io(io::Error),
    /// TOML 解析失败（保留供未来更严格的调用路径使用）；
    /// 当前 [`TomlFileStore::read`] 不会返回此变体——损坏文件改走「备份后
    /// 返回 `None`」分支。
    TomlParse(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "config file I/O error: {err}"),
            Self::TomlParse(msg) => write!(f, "config TOML parse error: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::TomlParse(_) => None,
        }
    }
}

impl From<io::Error> for ConfigError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// 配置存储抽象，便于 launcher 注入 [`TomlFileStore`]、单元测试注入内存实现。
pub trait LlmConfigStore: Send + Sync {
    /// 读取当前配置。
    ///
    /// 返回 `Ok(None)` 的三种情况（R9.5 / R10.2）：
    /// 1. 配置文件不存在；
    /// 2. `api_key` 为空字符串；
    /// 3. 文件存在但 TOML 解析失败（实现侧需把损坏文件备份为
    ///    `config.toml.bak.<unix_ns>`）。
    ///
    /// # Errors
    /// 仅在底层 I/O 失败（除常规的「文件不存在」之外）时返回 [`ConfigError`]。
    fn read(&self) -> Result<Option<LlmConfig>, ConfigError>;

    /// 写入配置；调用方需保证连通性测试已通过。
    ///
    /// # Errors
    /// 与 `%APPDATA%` 文件系统交互失败时返回 [`ConfigError::Io`]。
    fn write(&self, cfg: &LlmConfig) -> Result<(), ConfigError>;
}

/// 基于 TOML 文件的存储实现。
///
/// `path` 通常由 [`crate::launcher::paths::config_file_path`] 解析得到，
/// 即 `%APPDATA%\stats-code\config.toml`；测试环境可注入 `tempfile::TempDir`
/// 下的任意路径。
#[derive(Debug, Clone)]
pub struct TomlFileStore {
    pub path: PathBuf,
}

impl TomlFileStore {
    /// 用给定 TOML 文件路径构造存储实例。
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// 把损坏的配置文件 rename 为 `config.toml.bak.<unix_ns>`。
    ///
    /// 使用 `fs::rename` 而非 copy + delete：
    /// - 在同一文件系统下 rename 是原子的；
    /// - 不会留下两份相同内容的副本占空间。
    ///
    /// 时间戳用 `std::time::SystemTime` 取 Unix epoch 纳秒；不引入 `chrono`
    /// 等额外依赖（依赖最小化是本 crate 的明确目标）。
    ///
    /// 失败时尽量记录到备份路径但忽略错误——保证 [`Self::read`] 仍能返回
    /// `Ok(None)`，不阻塞 launcher 启动（R9.5）。
    fn backup_corrupt_file(&self) -> io::Result<PathBuf> {
        let ts_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let backup = backup_path_for(&self.path, ts_ns);
        fs::rename(&self.path, &backup)?;
        Ok(backup)
    }
}

/// 在原 TOML 路径同目录下生成 `<filename>.bak.<unix_ns>` 路径。
///
/// 抽出为独立函数主要服务于单元测试——可注入固定时间戳验证命名模板。
fn backup_path_for(original: &Path, unix_ns: u128) -> PathBuf {
    // `with_extension` 会把整段尾缀替换掉，因此手动追加扩展名以保留原 `.toml`。
    let mut name = original
        .file_name().map_or_else(|| std::ffi::OsString::from("config.toml"), std::ffi::OsStr::to_os_string);
    name.push(format!(".bak.{unix_ns}"));
    match original.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(name),
        _ => PathBuf::from(name),
    }
}

impl LlmConfigStore for TomlFileStore {
    fn read(&self) -> Result<Option<LlmConfig>, ConfigError> {
        // 1) 文件不存在 → 未配置（R9.5）。
        let raw = match fs::read_to_string(&self.path) {
            Ok(s) => s,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(ConfigError::Io(err)),
        };

        // 2) 解析 TOML。
        let parsed: Result<LlmConfig, _> = toml::from_str(&raw);
        let cfg = if let Ok(cfg) = parsed { cfg } else {
            // 损坏文件 → 备份并视同未配置（design.md Error Handling）。
            let _ = self.backup_corrupt_file();
            return Ok(None);
        };

        // 3) `api_key` 为空字符串视同未配置（R10.2）。
        if !cfg.is_configured() {
            return Ok(None);
        }
        Ok(Some(cfg))
    }

    fn write(&self, cfg: &LlmConfig) -> Result<(), ConfigError> {
        // 父目录可能尚不存在（首次写入）。launcher 会通过 `paths::ensure_app_data_dir`
        // 提前创建，但本函数也独立保障一次以便注入测试目录使用。
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(ConfigError::Io)?;
            }
        }
        let serialized =
            toml::to_string(cfg).map_err(|err| ConfigError::TomlParse(err.to_string()))?;
        fs::write(&self.path, serialized).map_err(ConfigError::Io)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir created")
    }

    #[test]
    fn provider_serializes_as_lowercase_string() {
        // 与前端 `LlmProvider = 'deepseek' | 'openai'` 对齐。
        let cfg = LlmConfig {
            provider: Provider::DeepSeek,
            api_key: "sk-abc".into(),
            base_url: None,
            model: None,
        };
        let toml_text = toml::to_string(&cfg).expect("serialize");
        assert!(
            toml_text.contains("provider = \"deepseek\""),
            "expected lowercase provider, got:\n{toml_text}"
        );

        let cfg2 = LlmConfig {
            provider: Provider::OpenAi,
            api_key: "sk-xyz".into(),
            base_url: None,
            model: None,
        };
        let toml_text2 = toml::to_string(&cfg2).expect("serialize");
        assert!(
            toml_text2.contains("provider = \"openai\""),
            "expected lowercase openai, got:\n{toml_text2}"
        );
    }

    #[test]
    fn read_returns_none_when_file_missing() {
        let dir = fixture_dir();
        let store = TomlFileStore::new(dir.path().join("config.toml"));
        assert!(store.read().expect("io ok").is_none());
    }

    #[test]
    fn read_returns_none_when_api_key_is_empty() {
        let dir = fixture_dir();
        let path = dir.path().join("config.toml");
        fs::write(&path, "provider = \"deepseek\"\napi_key = \"\"\n").expect("write");
        let store = TomlFileStore::new(path);
        assert!(store.read().expect("io ok").is_none());
    }

    #[test]
    fn read_returns_some_when_config_is_valid() {
        let dir = fixture_dir();
        let path = dir.path().join("config.toml");
        fs::write(&path, "provider = \"openai\"\napi_key = \"sk-test-1234\"\n").expect("write");
        let store = TomlFileStore::new(path);
        let cfg = store.read().expect("io ok").expect("present");
        assert_eq!(cfg.provider, Provider::OpenAi);
        assert_eq!(cfg.api_key, "sk-test-1234");
    }

    #[test]
    fn read_backs_up_malformed_file_and_returns_none() {
        let dir = fixture_dir();
        let path = dir.path().join("config.toml");
        // 故意写一段语法错误的 TOML（未闭合字符串）。
        fs::write(&path, "provider = \"deepseek\nthis-is-not-toml = ").expect("write");

        let store = TomlFileStore::new(path.clone());
        assert!(store.read().expect("io ok").is_none(), "应返回 None");

        // 原文件应已被改名走，备份至少有 1 份；`config.toml` 本身应不存在。
        assert!(
            !path.exists(),
            "损坏的 config.toml 应被 rename 为备份后从原位置移除"
        );
        let entries: Vec<_> = fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        let backups: Vec<_> = entries
            .iter()
            .filter(|n| n.starts_with("config.toml.bak."))
            .collect();
        assert_eq!(
            backups.len(),
            1,
            "应恰好生成一个备份，实际目录内容：{entries:?}"
        );
    }

    #[test]
    fn write_then_read_round_trips() {
        let dir = fixture_dir();
        let path = dir.path().join("config.toml");
        let store = TomlFileStore::new(path);

        let cfg = LlmConfig {
            provider: Provider::OpenAi,
            api_key: "sk-roundtrip".into(),
            base_url: Some("https://custom.openai.com/v1/".into()),
            model: Some("gpt-4o-custom".into()),
        };
        store.write(&cfg).expect("write ok");
        let read_back = store.read().expect("read ok").expect("some");
        assert_eq!(read_back, cfg);
    }

    #[test]
    fn write_creates_missing_parent_directory() {
        // launcher 通常会预创建 %APPDATA%\stats-code\，但测试覆盖一下兜底。
        let dir = fixture_dir();
        let nested = dir.path().join("nested").join("inner").join("config.toml");
        let store = TomlFileStore::new(nested.clone());

        store
            .write(&LlmConfig {
                provider: Provider::DeepSeek,
                api_key: "sk-creates-parent".into(),
                base_url: None,
                model: None,
            })
            .expect("write should mkdir -p parent");
        assert!(nested.exists(), "目标文件应已写入：{}", nested.display());
    }

    #[test]
    fn backup_path_template_is_filename_dot_bak_dot_unix_ns() {
        let original = PathBuf::from("/tmp/stats-code/config.toml");
        let backup = backup_path_for(&original, 1_700_000_000_123_456_789);
        assert_eq!(
            backup,
            PathBuf::from("/tmp/stats-code/config.toml.bak.1700000000123456789")
        );
    }

    #[test]
    fn config_error_io_implements_std_error_source() {
        // 确保 ConfigError 在调用方可以被 anyhow / `?` 链正常使用。
        let err: ConfigError = io::Error::new(io::ErrorKind::PermissionDenied, "denied").into();
        let std_err: &dyn std::error::Error = &err;
        assert!(std_err.source().is_some());
    }
}
