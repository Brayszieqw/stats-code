//! Prod 模式静态资源 handler（Requirement 6.2 / 6.3）。
//!
//! 仅在 `default` feature 下编译；`dev-vite` feature 开启时整个模块从编译
//! 单元中排除，由 launcher spawn 的 Vite_Dev_Server 提供前端资源（见
//! `crates/agent-server/src/lib.rs` 中的 `cfg` 守卫与 design.md）。
//!
//! 模块职责：
//! - 通过 [`rust_embed::RustEmbed`] 在编译期把仓库根的 `web/dist/` 嵌入
//!   二进制（`folder = "../../web/dist"` 是 `crates/agent-server/` 相对仓库
//!   根的路径）。
//! - [`serve`] 作为 axum router 的 `fallback` handler 处理任意未匹配的
//!   `/api/*` 之外的路径：
//!   1. 优先按 URI 路径在嵌入资源中精确匹配；
//!   2. 未命中则回落到 `index.html`，让前端 SPA 路由（React Router）能够
//!      处理刷新或深链接 URL；
//!   3. `index.html` 也缺失时 panic —— 这是「不应发生」的运行时违约：
//!      stats-code 顶层 build.rs 已在 prod 编译期校验 `web/dist/index.html`
//!      存在，agent-server 单独构建时若发生该 panic 表示 `web/dist/` 在
//!      编译后被人为破坏，系统无法继续提供前端服务。

use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::Response,
};
use rust_embed::RustEmbed;

/// 编译期嵌入的前端资源根。
///
/// 路径相对于 `crates/agent-server/Cargo.toml` 所在目录：`../../web/dist`
/// 指向仓库根的 `web/dist/`。`rust-embed` 在编译期把目录下所有文件读入
/// 二进制，因此发布二进制不依赖目标机器上存在 `web/dist/`。
#[derive(RustEmbed)]
#[folder = "../../web/dist"]
pub struct WebAssets;

/// SPA fallback handler。
///
/// 行为：
/// - 把请求 URI 的 path 去掉前导 `/` 作为 asset key 在 [`WebAssets`] 中查找；
/// - 命中 → 200 + 推断的 Content-Type + 文件原始字节；
/// - 未命中 → 回落到 `index.html`（200，`text/html`）；
/// - `index.html` 也缺失 → panic（违约场景，见模块文档）。
///
/// 该函数不区分目录请求与文件请求：例如 `/` 在 `WebAssets::get("")` 处必
/// 然返回 None，会自动落到 SPA fallback 取 `index.html`，行为正确。
#[allow(clippy::unused_async)] // axum handler 签名要求 async fn
pub async fn serve(uri: Uri) -> Response {
    let raw_path = uri.path().trim_start_matches('/');

    if let Some(file) = WebAssets::get(raw_path) {
        return build_response(raw_path, file.data.into_owned(), StatusCode::OK);
    }

    // SPA fallback：未命中时返回 index.html，让前端路由接管。
    let index = WebAssets::get("index.html").unwrap_or_else(|| {
        panic!(
            "embedded `web/dist/index.html` is missing — \
             prod build invariant violated (see Requirement 6.4)"
        )
    });

    build_response("index.html", index.data.into_owned(), StatusCode::OK)
}

/// 由文件路径推断 Content-Type 并组装 axum 响应。
///
/// `mime_guess` 根据扩展名给出 MIME；未知扩展默认 `application/octet-stream`，
/// 这是浏览器回退到「下载」的标准行为，不会导致 SPA 渲染异常。
fn build_response(path: &str, bytes: Vec<u8>, status: StatusCode) -> Response {
    let content_type = mime_guess::from_path(path)
        .first_or_octet_stream()
        .as_ref()
        .to_owned();

    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(bytes))
        .expect("static asset response builder is infallible for known headers")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `web/dist/index.html` 必须能从嵌入资源读出且非空。
    ///
    /// 这同时充当对 prod 构建前置条件的运行时探针：构建产出此 crate 的
    /// 测试二进制本身就要求 `web/dist/index.html` 存在；若 `web/dist/` 缺失，
    /// 编译已经在 `RustEmbed` derive 处失败，永远不会跑到这条断言。
    #[test]
    fn index_html_is_embedded_and_non_empty() {
        let file = WebAssets::get("index.html").expect("embedded index.html must exist");
        assert!(
            !file.data.is_empty(),
            "embedded index.html must have non-empty bytes"
        );
    }
}
