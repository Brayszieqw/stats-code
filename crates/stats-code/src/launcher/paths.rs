//! `%APPDATA%\stats-code\` 路径解析（Requirement 8.1, 9.4）。
//!
//! 暴露三个绝对路径访问器：
//! - [`app_data_dir`]：`%APPDATA%\stats-code\`
//! - [`lock_file_path`]：`%APPDATA%\stats-code\running.lock`
//! - [`config_file_path`]：`%APPDATA%\stats-code\config.toml`
//!
//! 以及一个副作用入口 [`ensure_app_data_dir`]：在缺失时创建该目录，权限完全
//! 依赖 NTFS 默认（仅当前用户可读，不显式 chmod / SetACL，对应 R9.4）。
//!
//! `%APPDATA%` 的解析委托给 [`dirs::config_dir`]，在 Windows 上等价于读取
//! `FOLDERID_RoamingAppData`（即 `%APPDATA%`，例如
//! `C:\Users\<user>\AppData\Roaming`）。其它平台（macOS、Linux）虽不在本特性
//! 支持矩阵内，但保持 API 在那些平台上仍能编译并返回与平台惯例相符的目录，
//! 便于在非 Windows 开发机上跑单元测试。

use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

/// `%APPDATA%` 下用于 stats-code 的子目录名。
pub const APP_DIR_NAME: &str = "stats-code";

/// `Lock_File` 文件名（位于 `%APPDATA%\stats-code\` 下）。
pub const LOCK_FILE_NAME: &str = "running.lock";

/// LLM 配置文件名（位于 `%APPDATA%\stats-code\` 下）。
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// 路径解析与创建过程中可能出现的错误。
#[derive(Debug, Error)]
pub enum PathsError {
    /// 操作系统不暴露用户级配置目录（Windows 下意味着 `%APPDATA%` 未设置）。
    #[error("无法解析 %APPDATA%：操作系统未暴露用户级配置目录")]
    AppDataUnavailable,

    /// 文件系统错误，附带触发该错误的路径以便诊断。
    #[error("访问 {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl PathsError {
    fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

/// 返回 `%APPDATA%\stats-code\` 的绝对路径。
///
/// 不创建目录、不检查存在性；如果只是为了拿到路径而不需要落盘，使用本函数。
/// 需要保证目录已存在时改用 [`ensure_app_data_dir`]。
///
/// # Errors
/// 当 [`dirs::config_dir`] 返回 `None`（拿不到 `%APPDATA%`）时返回
/// [`PathsError::AppDataUnavailable`]。
pub fn app_data_dir() -> Result<PathBuf, PathsError> {
    let base = dirs::config_dir().ok_or(PathsError::AppDataUnavailable)?;
    Ok(app_data_dir_in(&base))
}

/// 返回 `Lock_File`（`running.lock`）的绝对路径。
///
/// # Errors
/// 透传 [`app_data_dir`] 的错误。
pub fn lock_file_path() -> Result<PathBuf, PathsError> {
    Ok(app_data_dir()?.join(LOCK_FILE_NAME))
}

/// 返回 LLM 配置文件（`config.toml`）的绝对路径。
///
/// # Errors
/// 透传 [`app_data_dir`] 的错误。
pub fn config_file_path() -> Result<PathBuf, PathsError> {
    Ok(app_data_dir()?.join(CONFIG_FILE_NAME))
}

/// 确保 `%APPDATA%\stats-code\` 已存在并返回其绝对路径。
///
/// 如目录已存在则什么都不做，否则递归创建。本函数 **不** 显式调整 ACL，权限
/// 完全依赖 NTFS 默认继承（R9.4）：在 Windows 上 `%APPDATA%` 默认仅当前用户
/// 可读写。
///
/// # Errors
/// - [`PathsError::AppDataUnavailable`]：解析不到 `%APPDATA%`。
/// - [`PathsError::Io`]：创建目录时遇到文件系统错误（含路径已被占用为非目录
///   等情况）。
pub fn ensure_app_data_dir() -> Result<PathBuf, PathsError> {
    let dir = app_data_dir()?;
    ensure_dir(&dir)?;
    Ok(dir)
}

// --- 内部测试钩子 -----------------------------------------------------------
//
// 单元测试与 launcher 上层调用都不应直接构造 `%APPDATA%`，因此把「在某个 base
// 下计算 stats-code 子目录」抽成 `pub(crate)` 函数。这样测试里可以传一个
// `tempfile::TempDir` 当作 fake `%APPDATA%`，避免污染真实用户目录。

/// 在给定的基础目录下计算 `stats-code` 子目录路径。
///
/// 主要用于单元测试注入 fake `%APPDATA%`；生产路径由 [`app_data_dir`] 自己
/// 调用。
#[must_use]
pub(crate) fn app_data_dir_in(base: &Path) -> PathBuf {
    base.join(APP_DIR_NAME)
}

/// 在给定的基础目录下确保 `stats-code` 子目录存在。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn ensure_app_data_dir_in(base: &Path) -> Result<PathBuf, PathsError> {
    let dir = app_data_dir_in(base);
    ensure_dir(&dir)?;
    Ok(dir)
}

fn ensure_dir(dir: &Path) -> Result<(), PathsError> {
    match std::fs::create_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(err) => Err(PathsError::io(dir, err)),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        app_data_dir, app_data_dir_in, config_file_path, ensure_app_data_dir_in, lock_file_path,
        APP_DIR_NAME, CONFIG_FILE_NAME, LOCK_FILE_NAME,
    };

    #[test]
    fn app_data_dir_ends_with_stats_code() {
        let dir = app_data_dir().expect("APPDATA 应可解析");
        assert_eq!(dir.file_name().and_then(|s| s.to_str()), Some(APP_DIR_NAME));
        // 必须是绝对路径，否则单实例 / 配置写入会落到 CWD 下。
        assert!(
            dir.is_absolute(),
            "app_data_dir 必须返回绝对路径，实际：{}",
            dir.display()
        );
    }

    #[test]
    fn lock_and_config_paths_live_in_app_data_dir() {
        let base = app_data_dir().expect("APPDATA 应可解析");
        let lock = lock_file_path().expect("APPDATA 应可解析");
        let cfg = config_file_path().expect("APPDATA 应可解析");

        assert_eq!(lock.parent(), Some(base.as_path()));
        assert_eq!(cfg.parent(), Some(base.as_path()));
        assert_eq!(lock.file_name().and_then(|s| s.to_str()), Some(LOCK_FILE_NAME));
        assert_eq!(cfg.file_name().and_then(|s| s.to_str()), Some(CONFIG_FILE_NAME));
    }

    #[test]
    fn ensure_creates_missing_directory_under_fake_appdata() {
        let tmp = tempfile::tempdir().expect("能创建临时目录");
        let expected = app_data_dir_in(tmp.path());
        assert!(!expected.exists(), "夹具应从不存在的子目录开始");

        let created = ensure_app_data_dir_in(tmp.path()).expect("应能创建目录");
        assert_eq!(created, expected);
        assert!(created.is_dir(), "创建后应为目录");
    }

    #[test]
    fn ensure_is_idempotent() {
        let tmp = tempfile::tempdir().expect("能创建临时目录");

        let first = ensure_app_data_dir_in(tmp.path()).expect("首次创建");
        let second = ensure_app_data_dir_in(tmp.path()).expect("重入应成功");

        assert_eq!(first, second);
        assert!(second.is_dir());
    }
}
