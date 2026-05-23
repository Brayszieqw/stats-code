//! Build script for `stats-code`.
//!
//! Feature: single-command-launcher
//! Requirement 6.4 — prod 模式构建时如果 `web/dist/` 不存在，编译过程必须失败
//! 并输出明确的诊断信息。
//!
//! 行为：
//! - 默认（`dev-vite` feature 关闭，即 prod）：校验 `<crate-root>/../../web/dist/`
//!   目录存在且包含 `index.html`，否则用 `cargo:warning=` 输出诊断并 panic。
//! - `dev-vite` feature 开启：跳过校验（dev 模式下前端由 Vite 子进程提供，
//!   不依赖 `web/dist/`）。
//!
//! 不引入额外依赖，仅使用 std。

use std::path::PathBuf;

fn main() {
    // 重新运行触发条件：build.rs 自身、相关环境变量、以及 web/dist 内容变化。
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_DEV_VITE");

    // dev-vite 开启时跳过校验。Cargo 会把启用的 feature 暴露为
    // `CARGO_FEATURE_<FEATURE_NAME_UPPER_SNAKE_CASE>` 环境变量。
    if std::env::var_os("CARGO_FEATURE_DEV_VITE").is_some() {
        return;
    }

    // 解析 web/dist 路径：从 crates/stats-code/ 向上两级到仓库根。
    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR must be set by Cargo"),
    );
    let web_dist = manifest_dir
        .join("..")
        .join("..")
        .join("web")
        .join("dist");
    let index_html = web_dist.join("index.html");

    // 监听这两条路径的变更，便于增量重建在 web 重新构建后触发 rebuild。
    println!("cargo:rerun-if-changed={}", web_dist.display());
    println!("cargo:rerun-if-changed={}", index_html.display());

    if !web_dist.is_dir() {
        println!(
            "cargo:warning=web/dist directory not found at {}",
            web_dist.display()
        );
        panic!(
            "stats-code prod build requires `web/dist/` to exist (Requirement 6.4). \
             Run `npm run build` in `web/` first, or build with `--features dev-vite` \
             for the dev workflow. Expected path: {}",
            web_dist.display()
        );
    }

    if !index_html.is_file() {
        println!(
            "cargo:warning=web/dist/index.html missing at {}",
            index_html.display()
        );
        panic!(
            "stats-code prod build requires `web/dist/index.html` (Requirement 6.4). \
             The directory exists but the entry point is missing — re-run `npm run build` \
             in `web/`. Expected path: {}",
            index_html.display()
        );
    }
}
