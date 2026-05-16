use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const BRAILLE_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const ASCII_FRAMES: &[char] = &['|', '/', '-', '\\'];

pub struct AnimatedIndicator {
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) handle: Option<JoinHandle<()>>,
}

impl AnimatedIndicator {
    /// Return the current frame for a Unicode spinner (0–9).
    #[must_use]
    pub fn braille_frame(frame: usize) -> char {
        BRAILLE_FRAMES[frame % BRAILLE_FRAMES.len()]
    }

    /// Return the current frame for an ASCII spinner (0–3).
    #[must_use]
    pub fn ascii_frame(frame: usize) -> char {
        ASCII_FRAMES[frame % ASCII_FRAMES.len()]
    }
}

/// Start an animated spinner printing to `out`.
///
/// `text` is displayed next to the spinner character.
/// When `caps.supports_unicode` is true Braille frames are used; otherwise
/// ASCII `|/-\` frames.
#[must_use]
pub fn start_spinner(text: String, supports_unicode: bool) -> AnimatedIndicator {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop);

    let handle = thread::spawn(move || {
        let mut stdout = io::stdout();
        let frames: &[char] = if supports_unicode {
            BRAILLE_FRAMES
        } else {
            ASCII_FRAMES
        };
        let mut idx = 0usize;
        while !stop_clone.load(Ordering::Relaxed) {
            let frame = frames[idx % frames.len()];
            let _ = write!(stdout, "\r\x1b[K{frame} {text}");
            let _ = stdout.flush();
            idx = idx.wrapping_add(1);
            thread::sleep(Duration::from_millis(80));
        }
        // Clear the line
        let _ = write!(stdout, "\r\x1b[2K");
        let _ = stdout.flush();
    });

    AnimatedIndicator {
        stop,
        handle: Some(handle),
    }
}

impl AnimatedIndicator {
    /// Stop the spinner, clear its line, and join the thread.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for AnimatedIndicator {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn braille_frames_cycle() {
        assert_eq!(AnimatedIndicator::braille_frame(0), '⠋');
        assert_eq!(AnimatedIndicator::braille_frame(10), '⠋');
    }

    #[test]
    fn ascii_frames_cycle() {
        assert_eq!(AnimatedIndicator::ascii_frame(0), '|');
        assert_eq!(AnimatedIndicator::ascii_frame(4), '|');
    }
}
