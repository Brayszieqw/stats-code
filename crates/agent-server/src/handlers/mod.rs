//! HTTP request handlers for the agent server.

pub mod audio;
pub mod dataset;
pub mod llm_config;
pub mod message;
pub mod session;

/// Prod 模式静态资源 handler（Requirement 6.2 / 6.3）。
///
/// `dev-vite` feature 开启时整个模块从编译单元中排除：dev 模式下前端由
/// launcher spawn 的 Vite_Dev_Server 直接伺服，agent-server 不嵌入也不
/// 触发对 `web/dist/` 目录的读取，避免在没有运行 `npm run build` 的开发
/// 环境下编译失败。
#[cfg(not(feature = "dev-vite"))]
pub mod static_assets;
