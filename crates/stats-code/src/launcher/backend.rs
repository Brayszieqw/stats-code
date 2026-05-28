//! `Agent_Backend` 启动骨架（占位）。
//!
//! 真正的实现在 task 2.3 / 2.5 / 8.1 中完成；当前仅声明纯函数 / 防御性
//! 校验的签名，让模块树编译。

use std::io;
use std::net::TcpListener;

/// 启动后的运行模式标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// prod：前端通过 rust-embed 内嵌伺服。
    Prod,
    /// dev：前端通过外部 `Vite_Dev_Server` 子进程提供。
    Dev,
}

/// 拼装写入 stdout 的启动日志行。
///
/// 纯函数：根据 [`RunMode`] 与实际 bind 的 backend 端口（`port`，即
/// design.md 中的 `Actual_Port`）拼出展示给用户的启动日志。
///
/// - **prod**：用户可见 URL 即 `Agent_Backend` URL，形如
///   `http://127.0.0.1:<port>/`。
/// - **dev**：用户可见 URL 是 `Vite_Dev_Server` 的 `http://127.0.0.1:5173/`
///   （由 `npm run dev` 默认监听端口，并把 `/api/*` 反代到 backend），
///   backend 端口仅作信息性标注随后附上。
///
/// 该函数被 design.md 的 *Property 3: 启动 URL 与日志包含 `Actual_Port`*
/// 约束：用 `url::Url::parse` 解析返回值中第一个出现的 URL 时
/// - `host_str` 必为 `Some("127.0.0.1")`
/// - path 必为 `"/"`
/// - `port_or_known_default` 必为 `Some(port)`（prod）或 `Some(5173)`（dev）
///
/// _Validates: Requirements 1.5, 4.4, 5.1, 5.2_
#[must_use]
pub fn format_ready_line(port: u16, mode: RunMode) -> String {
    match mode {
        RunMode::Prod => {
            format!("stats-code listening on http://127.0.0.1:{port}/ (prod)")
        }
        RunMode::Dev => {
            // dev 模式下 Vite URL 在前（用户可见入口），backend URL 跟在
            // 括号里作为信息性标注；两者都包含 `http://127.0.0.1:<...>/`，
            // 且 dev 字符串中必然出现 `5173`。
            format!(
                "stats-code listening on http://127.0.0.1:5173/ (dev, backend on http://127.0.0.1:{port}/)"
            )
        }
    }
}

/// 启动 `Agent_Backend（占位`）。
///
/// # Errors
/// 真实实现会把 `axum::serve` 的运行错误回传。当前占位实现立即返回 `Ok(())`
/// 而不消费 listener。
pub fn serve(_listener: TcpListener) -> io::Result<()> {
    Ok(())
}

/// 防御性校验：只允许 `127.0.0.1` 作为 bind host。
///
/// 任何非 loopback host 都会触发 panic，符合 design.md 中的
/// `BindError::NonLoopback` 处理策略。
pub fn assert_loopback_bind(host: &str) {
    assert!(
        host == "127.0.0.1",
        "Stats Code launcher refuses to bind on non-loopback host: {host}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Happy path：`127.0.0.1` 是唯一被允许的 bind host。
    ///
    /// _Validates: Requirements 3.2_
    #[test]
    fn allows_loopback_host() {
        assert_loopback_bind("127.0.0.1");
    }

    /// `0.0.0.0` 会把服务暴露到所有网卡，必须被拒绝。
    ///
    /// _Validates: Requirements 3.2_
    #[test]
    #[should_panic(expected = "refuses to bind on non-loopback host: 0.0.0.0")]
    fn rejects_wildcard_host() {
        assert_loopback_bind("0.0.0.0");
    }

    /// 外部网卡 IP（局域网地址）也必须被拒绝。
    ///
    /// _Validates: Requirements 3.2_
    #[test]
    #[should_panic(expected = "refuses to bind on non-loopback host: 192.168.1.10")]
    fn rejects_external_lan_host() {
        assert_loopback_bind("192.168.1.10");
    }

    /// IPv6 loopback `::1` 在当前实现下也不被允许；防御性校验只放行
    /// 字符串字面量 `127.0.0.1`，避免 host 解析歧义。
    ///
    /// _Validates: Requirements 3.2_
    #[test]
    #[should_panic(expected = "refuses to bind on non-loopback host: ::1")]
    fn rejects_ipv6_loopback_literal() {
        assert_loopback_bind("::1");
    }
}
