//! Encoding detection and decoding for uploaded data files.
//!
//! Detection order (R3.6): UTF-8 → GBK → UTF-16.

use crate::models::Encoding;

/// Error returned when bytes cannot be decoded by any supported encoding.
#[derive(Debug, Clone, thiserror::Error)]
#[error("unable to detect encoding: bytes could not be decoded as UTF-8, GBK, or UTF-16")]
pub struct EncodingDetectError;

/// Detect the encoding of `bytes` and decode them into a `String`.
///
/// Detection order:
/// 1. UTF-8 (`std::str::from_utf8`)
/// 2. GBK (`encoding_rs::GBK` without BOM handling and without replacement)
/// 3. UTF-16 (check BOM for LE/BE, then try both without BOM)
///
/// Returns `(decoded_string, detected_encoding)` on success.
pub fn detect_and_decode(bytes: &[u8]) -> Result<(String, Encoding), EncodingDetectError> {
    // 1. Try UTF-8
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Ok((s.to_owned(), Encoding::Utf8));
    }

    // 2. Try GBK (without replacement — returns None if unmappable bytes exist)
    if let Some(decoded) =
        encoding_rs::GBK.decode_without_bom_handling_and_without_replacement(bytes)
    {
        return Ok((decoded.into_owned(), Encoding::Gbk));
    }

    // 3. Try UTF-16
    if let Some(result) = try_utf16(bytes) {
        return Ok((result, Encoding::Utf16));
    }

    Err(EncodingDetectError)
}

/// Attempt to decode bytes as UTF-16.
///
/// Strategy:
/// - If BOM present (FF FE → LE, FE FF → BE), use corresponding decoder.
/// - Otherwise try both LE and BE without replacement.
fn try_utf16(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 {
        return None;
    }

    // Check BOM — skip the 2-byte BOM prefix before decoding
    if bytes[0] == 0xFF && bytes[1] == 0xFE {
        // UTF-16 LE BOM
        return decode_utf16_without_replacement(encoding_rs::UTF_16LE, &bytes[2..]);
    }
    if bytes[0] == 0xFE && bytes[1] == 0xFF {
        // UTF-16 BE BOM
        return decode_utf16_without_replacement(encoding_rs::UTF_16BE, &bytes[2..]);
    }

    // No BOM — need even byte length for valid UTF-16
    if !bytes.len().is_multiple_of(2) {
        return None;
    }

    // Try LE first, then BE
    if let Some(s) = decode_utf16_without_replacement(encoding_rs::UTF_16LE, bytes) {
        return Some(s);
    }
    if let Some(s) = decode_utf16_without_replacement(encoding_rs::UTF_16BE, bytes) {
        return Some(s);
    }

    None
}

/// Decode bytes using the given UTF-16 encoding without replacement characters.
fn decode_utf16_without_replacement(
    encoding: &'static encoding_rs::Encoding,
    bytes: &[u8],
) -> Option<String> {
    let (result, _had_errors) = encoding.decode_without_bom_handling(bytes);
    // If the decoded string contains the replacement character, decoding was lossy
    if result.contains('\u{FFFD}') {
        return None;
    }
    Some(result.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf8_ascii() {
        let input = b"hello world";
        let (decoded, enc) = detect_and_decode(input).unwrap();
        assert_eq!(decoded, "hello world");
        assert_eq!(enc, Encoding::Utf8);
    }

    #[test]
    fn test_utf8_chinese() {
        let input = "你好世界".as_bytes();
        let (decoded, enc) = detect_and_decode(input).unwrap();
        assert_eq!(decoded, "你好世界");
        assert_eq!(enc, Encoding::Utf8);
    }

    #[test]
    fn test_gbk_chinese() {
        // "你好" in GBK: C4 E3 BA C3
        let gbk_bytes: &[u8] = &[0xC4, 0xE3, 0xBA, 0xC3];
        let (decoded, enc) = detect_and_decode(gbk_bytes).unwrap();
        assert_eq!(decoded, "你好");
        assert_eq!(enc, Encoding::Gbk);
    }

    #[test]
    fn test_utf16le_with_bom() {
        // UTF-16 LE BOM + "AB"
        let bytes: Vec<u8> = vec![0xFF, 0xFE, 0x41, 0x00, 0x42, 0x00];
        let (decoded, enc) = detect_and_decode(&bytes).unwrap();
        assert_eq!(decoded, "AB");
        assert_eq!(enc, Encoding::Utf16);
    }

    #[test]
    fn test_utf16be_with_bom() {
        // UTF-16 BE BOM + "AB"
        let bytes: Vec<u8> = vec![0xFE, 0xFF, 0x00, 0x41, 0x00, 0x42];
        let (decoded, enc) = detect_and_decode(&bytes).unwrap();
        assert_eq!(decoded, "AB");
        assert_eq!(enc, Encoding::Utf16);
    }

    #[test]
    fn test_invalid_bytes() {
        // Single byte 0xFF is not valid UTF-8, not valid GBK (alone), not valid UTF-16
        let bytes: &[u8] = &[0xFF];
        assert!(detect_and_decode(bytes).is_err());
    }
}
