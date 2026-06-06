//! Validation functions for user messages and audio uploads.

use crate::models::{ErrorCode, ErrorPayload};

/// Maximum allowed character count for a text message (R1.4).
const MAX_MESSAGE_CHARS: usize = 8000;

/// Maximum allowed audio duration in seconds (R2.5).
const MAX_AUDIO_DURATION_SECS: u32 = 60;

/// Maximum allowed audio file size in bytes (10 MB) (R2.5).
const MAX_AUDIO_SIZE_BYTES: u64 = 10 * 1024 * 1024;

/// Validates that a text message does not exceed the 8000-character limit.
///
/// Returns `Ok(())` if the message length (in Unicode scalar values) is ≤ 8000.
/// Returns `Err(ErrorPayload)` with `ErrorCode::MessageTooLong` otherwise,
/// including the actual character count and the limit in the Chinese error message.
pub fn validate_message_length(s: &str) -> Result<(), ErrorPayload> {
    let actual = s.chars().count();
    if actual > MAX_MESSAGE_CHARS {
        Err(ErrorPayload {
            error_code: ErrorCode::MessageTooLong,
            message: format!(
                "消息过长：当前 {actual} 字，超过上限 {MAX_MESSAGE_CHARS} 字"
            ),
            details: None,
        })
    } else {
        Ok(())
    }
}

/// Validates that an audio upload does not exceed duration or size limits.
///
/// Returns `Ok(())` if `duration_secs <= 60` **and** `size_bytes <= 10 MB`.
/// Returns `Err(ErrorPayload)` with `ErrorCode::AudioTooLarge` otherwise.
pub fn validate_audio(duration_secs: u32, size_bytes: u64) -> Result<(), ErrorPayload> {
    if duration_secs > MAX_AUDIO_DURATION_SECS || size_bytes > MAX_AUDIO_SIZE_BYTES {
        let max_mb = MAX_AUDIO_SIZE_BYTES / (1024 * 1024);
        Err(ErrorPayload {
            error_code: ErrorCode::AudioTooLarge,
            message: format!(
                "录音不能超过 {MAX_AUDIO_DURATION_SECS} 秒或 {max_mb} MB（当前：{duration_secs} 秒，{size_bytes} 字节）"
            ),
            details: None,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_within_limit() {
        let s = "a".repeat(8000);
        assert!(validate_message_length(&s).is_ok());
    }

    #[test]
    fn message_at_boundary_ok() {
        // Exactly 8000 chars should pass
        let s: String = std::iter::repeat_n('中', 8000).collect();
        assert!(validate_message_length(&s).is_ok());
    }

    #[test]
    fn message_exceeds_limit() {
        let s: String = std::iter::repeat_n('字', 8001).collect();
        let err = validate_message_length(&s).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::MessageTooLong);
        assert!(err.message.contains("8001"));
        assert!(err.message.contains("8000"));
        // Must contain Chinese characters
        assert!(err.message.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
    }

    #[test]
    fn empty_message_ok() {
        assert!(validate_message_length("").is_ok());
    }

    #[test]
    fn audio_within_limits() {
        assert!(validate_audio(60, 10 * 1024 * 1024).is_ok());
    }

    #[test]
    fn audio_duration_exceeds() {
        let err = validate_audio(61, 0).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::AudioTooLarge);
        assert!(err.message.contains("60"));
    }

    #[test]
    fn audio_size_exceeds() {
        let err = validate_audio(30, 10 * 1024 * 1024 + 1).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::AudioTooLarge);
        assert!(err.message.contains("10 MB"));
    }

    #[test]
    fn audio_both_exceed() {
        let err = validate_audio(120, 20 * 1024 * 1024).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::AudioTooLarge);
    }

    #[test]
    fn audio_zero_values_ok() {
        assert!(validate_audio(0, 0).is_ok());
    }
}
