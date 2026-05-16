use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::spinner::AnimatedIndicator;

const TRACK: u8 = 20;
const BLOCK: u8 = 3;

const UNICODE_BLOCKS: &[char] = &['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];

/// Pure function: given a frame counter, track width, and block width,
/// return the 0-indexed start position of the bouncing block.
#[must_use]
pub fn bounce_position(frame: u64, track: u8, block: u8) -> u8 {
    let range = u64::from(track - block);
    if range == 0 {
        return 0;
    }
    let period = range * 2;
    let pos = frame % period;
    if pos < range {
        pos as u8
    } else {
        (period - pos) as u8
    }
}

#[must_use]
pub fn start_progress(prefix: String, supports_unicode: bool) -> AnimatedIndicator {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop);

    let handle = thread::spawn(move || {
        let mut stdout = io::stdout();
        let mut idx = 0u64;
        while !stop_clone.load(Ordering::Relaxed) {
            let pos = bounce_position(idx, TRACK, BLOCK);
            if supports_unicode {
                let mut bar = String::with_capacity(TRACK as usize + 2);
                bar.push('[');
                for i in 0..TRACK {
                    if i >= pos && i < pos + BLOCK {
                        bar.push(UNICODE_BLOCKS[7]);
                    } else {
                        bar.push(' ');
                    }
                }
                bar.push(']');
                let _ = write!(stdout, "\r\x1b[K{bar} {prefix}");
            } else {
                let mut bar = String::with_capacity(TRACK as usize + 2);
                bar.push('[');
                for i in 0..TRACK {
                    if i >= pos && i < pos + BLOCK {
                        bar.push('=');
                    } else {
                        bar.push(' ');
                    }
                }
                bar.push(']');
                let _ = write!(stdout, "\r\x1b[K{bar} {prefix}");
            }
            let _ = stdout.flush();
            idx = idx.wrapping_add(1);
            thread::sleep(Duration::from_millis(100));
        }
        let _ = write!(stdout, "\r\x1b[2K");
        let _ = stdout.flush();
    });

    AnimatedIndicator {
        stop,
        handle: Some(handle),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounce_in_range() {
        for frame in 0..200 {
            let pos = bounce_position(frame, 20, 3);
            assert!(pos <= 17, "pos {pos} > track-block (17) at frame {frame}");
        }
    }

    #[test]
    fn bounce_periodic() {
        let p0 = bounce_position(0, 20, 3);
        let period = 2 * (20 - 3) as u64;
        let p_period = bounce_position(period, 20, 3);
        assert_eq!(p0, p_period);
    }

    #[test]
    fn bounce_zero_range() {
        assert_eq!(bounce_position(0, 3, 3), 0);
        assert_eq!(bounce_position(10, 3, 3), 0);
    }
}
