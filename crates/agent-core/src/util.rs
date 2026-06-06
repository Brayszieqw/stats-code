//! Utility functions for the agent-core crate.

use sha2::{Digest, Sha256};

/// Compute the SHA-256 digest of `bytes` and return it as a 64-character
/// lowercase hexadecimal string.
///
/// The computation is deterministic: identical byte sequences always produce
/// identical output regardless of host or time (Requirement 1.1, 1.5).
#[must_use]
pub fn sha256_hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(bytes);
    digest.iter().fold(String::with_capacity(64), |mut acc, b| {
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

/// Extract a UTF-8 safe excerpt from stderr bytes.
///
/// Guarantees:
/// 1. The returned string's byte length ≤ `max_len`
/// 2. The returned string is valid UTF-8 (never truncates in the middle of a multi-byte sequence)
/// 3. If `bytes` is valid UTF-8 and its byte length ≤ `max_len`, the returned string equals the
///    original bytes decoded
///
/// Algorithm:
/// 1. Try to decode bytes as UTF-8. If it fails, use `String::from_utf8_lossy`
/// 2. If the resulting string's byte length ≤ `max_len`, return it as-is
/// 3. Otherwise, find the largest index ≤ `max_len` that is a char boundary and truncate there
#[must_use] 
pub fn stderr_excerpt(bytes: &[u8], max_len: usize) -> String {
    let s = match std::str::from_utf8(bytes) {
        Ok(valid) => valid.to_owned(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    };

    if s.len() <= max_len {
        return s;
    }

    // Find the largest index <= max_len that is a char boundary
    let mut end = max_len;
    while !s.is_char_boundary(end) {
        end -= 1;
    }

    s[..end].to_owned()
}

/// Convenience wrapper with default `max_len` of 4096 bytes.
#[must_use] 
pub fn stderr_excerpt_default(bytes: &[u8]) -> String {
    stderr_excerpt(bytes, 4096)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_empty_input() {
        // SHA-256 of empty bytes is the well-known constant
        let result = sha256_hex_lower(b"");
        assert_eq!(
            result,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(result.len(), 64);
    }

    #[test]
    fn sha256_deterministic() {
        let input = b"hello world";
        let a = sha256_hex_lower(input);
        let b = sha256_hex_lower(input);
        assert_eq!(a, b);
    }

    #[test]
    fn sha256_known_value() {
        // SHA-256("hello world") is a well-known test vector
        let result = sha256_hex_lower(b"hello world");
        assert_eq!(
            result,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn sha256_is_lowercase_hex_64_chars() {
        let result = sha256_hex_lower(b"arbitrary bytes \x00\xFF");
        assert_eq!(result.len(), 64);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn ascii_within_limit() {
        let input = b"hello world";
        assert_eq!(stderr_excerpt(input, 100), "hello world");
    }

    #[test]
    fn ascii_truncated() {
        let input = b"hello world";
        assert_eq!(stderr_excerpt(input, 5), "hello");
    }

    #[test]
    fn multibyte_not_split() {
        // "你好" is 6 bytes (3 per char), truncating at 4 should give "你" (3 bytes)
        let input = "你好".as_bytes();
        let result = stderr_excerpt(input, 4);
        assert_eq!(result, "你");
        assert!(result.len() <= 4);
    }

    #[test]
    fn exact_boundary() {
        // "你好" is 6 bytes, max_len=6 should return full string
        let input = "你好".as_bytes();
        assert_eq!(stderr_excerpt(input, 6), "你好");
    }

    #[test]
    fn invalid_utf8_uses_lossy() {
        let input: &[u8] = &[0xFF, 0xFE, b'h', b'i'];
        let result = stderr_excerpt(input, 100);
        assert!(result.contains("hi"));
        assert!(result.is_char_boundary(result.len()));
    }

    #[test]
    fn empty_input() {
        assert_eq!(stderr_excerpt(b"", 100), "");
    }

    #[test]
    fn zero_max_len() {
        assert_eq!(stderr_excerpt(b"hello", 0), "");
    }

    #[test]
    fn default_uses_4096() {
        let input = vec![b'a'; 5000];
        let result = stderr_excerpt_default(&input);
        assert_eq!(result.len(), 4096);
    }
}
