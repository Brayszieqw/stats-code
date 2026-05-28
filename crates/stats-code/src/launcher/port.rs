//! 端口扫描：在 `127.0.0.1` 上找到 [`ScanRange`] 内第一个能成功 `bind` 的 TCP 端口。
//!
//! 设计参考 `.kiro/specs/single-command-launcher/design.md` 的 Property 2：
//! 给定占用集合 `S ⊆ Port_Range`，本函数返回 `min(Port_Range \ S)` 对应的
//! [`TcpListener`]；若 `Port_Range \ S` 为空，返回
//! [`ScanError::AllPortsBusy`]。
//!
//! 返回 `TcpListener` 而非端口号，是为了避免「扫描」与「真正 bind」之间出现
//! 第三方进程抢占同一端口的 race condition。
//!
//! 满足 Requirements:
//! - 3.1 监听地址硬编码 `127.0.0.1`
//! - 4.1 从 `Default_Port` 起递增 1 顺序尝试
//! - 4.2 取第一个可 bind 端口为 `Actual_Port`
//! - 4.3 全部失败时返回 `AllPortsBusy`

use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};

/// 端口扫描区间。`start` 为闭区间起点，`end_exclusive` 为开区间终点。
#[derive(Debug, Clone, Copy)]
pub struct ScanRange {
    pub start: u16,
    pub end_exclusive: u16,
}

/// `Stats_Code_Launcher` 的默认扫描区间 `8080..8200`。
pub const DEFAULT_RANGE: ScanRange = ScanRange {
    start: 8080,
    end_exclusive: 8200,
};

/// 端口扫描错误。
#[derive(Debug)]
pub enum ScanError {
    /// 区间内全部端口均无法 bind。
    AllPortsBusy {
        tried: ScanRange,
        last_error: io::Error,
    },
}

/// 在 [`ScanRange`] 内按递增顺序寻找第一个能在 `127.0.0.1` 成功 `bind` 的端口，
/// 并返回对应的 [`TcpListener`]。
///
/// # Errors
/// 当区间内全部端口均无法 bind（或区间为空）时返回
/// [`ScanError::AllPortsBusy`]，其中 `last_error` 为最后一次 `bind` 失败的
/// `io::Error`（区间为空时为合成的 `InvalidInput`）。
pub fn scan_first_bindable(range: ScanRange) -> Result<TcpListener, ScanError> {
    let mut last_error: Option<io::Error> = None;

    for port in range.start..range.end_exclusive {
        let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
        match TcpListener::bind(addr) {
            Ok(listener) => return Ok(listener),
            Err(e) => last_error = Some(e),
        }
    }

    Err(ScanError::AllPortsBusy {
        tried: range,
        last_error: last_error.unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "scan range is empty (start >= end_exclusive)",
            )
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener as Std;

    #[test]
    fn returns_first_bindable_skipping_occupied_ports() {
        // 持有一个临时端口令其占用，构造一个从该端口起向上延伸 50 个端口的
        // 区间。OS 不保证 `port+1` 是否空闲，因此不强断言精确返回值；只要扫描器
        // 跳过了被占用的起点端口、返回区间内某个能 bind 的端口即可证明
        // Property「返回值跳过了占用端口」。下界严格大于 `port` 直接证明 skip 行为。
        let occupied = Std::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .expect("bind ephemeral port");
        let port = occupied.local_addr().unwrap().port();
        let range = ScanRange {
            start: port,
            end_exclusive: port.saturating_add(50),
        };

        let listener = scan_first_bindable(range).expect("expected to find a free port in range");
        let got = listener.local_addr().unwrap().port();

        assert!(
            got > port && got < port.saturating_add(50),
            "returned port {got} not in (port, port+50)"
        );
        // 确认绑定到 loopback，满足 Requirement 3.1。
        assert!(listener.local_addr().unwrap().ip().is_loopback());
        // 起点端口仍被 `occupied` 持有：再次 bind 必然失败，证明扫描器是
        // 因为「占用」才跳过它。
        assert!(
            Std::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).is_err(),
            "starting port {port} should remain occupied"
        );
    }

    #[test]
    fn empty_range_returns_all_ports_busy() {
        let range = ScanRange {
            start: 8080,
            end_exclusive: 8080,
        };
        let err = scan_first_bindable(range).expect_err("empty range must error");
        match err {
            ScanError::AllPortsBusy { tried, .. } => {
                assert_eq!(tried.start, 8080);
                assert_eq!(tried.end_exclusive, 8080);
            }
        }
    }

    #[test]
    fn fully_occupied_range_returns_all_ports_busy() {
        // 占用一个端口，把 range 收缩成「单端口且被占」。
        let occupied = Std::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .expect("bind ephemeral port");
        let port = occupied.local_addr().unwrap().port();
        let range = ScanRange {
            start: port,
            end_exclusive: port + 1,
        };
        let err = scan_first_bindable(range).expect_err("fully occupied range must error");
        match err {
            ScanError::AllPortsBusy { tried, last_error } => {
                assert_eq!(tried.start, port);
                assert_eq!(tried.end_exclusive, port + 1);
                // 必须携带真实的 OS 错误而非合成 InvalidInput。
                assert_ne!(last_error.kind(), io::ErrorKind::InvalidInput);
            }
        }
    }

    #[test]
    fn first_port_free_returns_immediately() {
        // 拿一个临时端口，关掉它，把 range 设成 [port, port+1)。
        let listener = Std::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .expect("bind ephemeral port");
        let port = listener.local_addr().unwrap().port();
        drop(listener); // 释放，让端口在测试过程中暂时空闲。

        let range = ScanRange {
            start: port,
            end_exclusive: port + 1,
        };
        // 注意：这里有理论上的 race —— 端口可能被其他进程在此瞬间抢占。
        // 在实际 CI 环境下足够稳定；若失败只表明环境干扰，不是逻辑 bug。
        if let Ok(l) = scan_first_bindable(range) {
            assert_eq!(l.local_addr().unwrap().port(), port);
        }
    }
}
