//! Deterministic zip writer for the Audit Snapshot.
//!
//! Implements task 6.6 of `parity-and-multilang-sidecar`. See
//! `design.md` §6 ("Snapshot Exporter") for the determinism contract and
//! Requirement 7.1.
//!
//! Determinism contract:
//! - mtime fixed to `1980-01-01T00:00:00Z` (the ZIP epoch — DOS time
//!   `0x0021 0x0000`),
//! - `Stored` compression (no DEFLATE) so byte-identical inputs produce
//!   byte-identical zip bytes,
//! - entries sorted by archive name (lexicographic on raw bytes) before
//!   writing,
//! - no system clock, environment, or randomness is consulted.
//!
//! Atomicity:
//! - the writer creates `dest_tmp`, writes the full archive, calls
//!   `File::sync_all()` and drops the file handle,
//! - on **any** error the partial `dest_tmp` is removed before returning,
//! - this function does **not** rename to the final destination; the
//!   caller (task 6.7) performs the atomic rename to the user-chosen `.zip`
//!   path.
//!
//! Format:
//! - hand-rolled ZIP layout: a sequence of `Local File Header + file data`
//!   records, followed by a Central Directory of equivalent entries, ending
//!   with an "End of Central Directory" (EOCD) record. Only `Stored`
//!   compression is emitted; no Zip64, no DEFLATE, no encryption.
//! - all multibyte fields are little-endian per the ZIP spec.
//! - CRC32 is the IEEE polynomial (`0xedb88320` reflected) used by ZIP.
//!
//! _Requirements: 7.1_

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

/// One in-memory file destined for the snapshot zip.
///
/// `name` is the archive path inside the zip and **must** use forward-slash
/// (`/`) separators only. UTF-8 is enforced by the `String` type. Names that
/// are empty, start with `/`, contain `\`, or contain a `..` path component
/// are rejected at write time as `InvalidEntry`.
///
/// `bytes` is the file payload, written verbatim (no encoding, no BOM).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntry {
    /// Archive path inside the zip, forward-slash separators only.
    pub name: String,
    /// File payload bytes.
    pub bytes: Vec<u8>,
}

/// Error returned by the deterministic zip writer.
#[derive(Debug, thiserror::Error)]
pub enum ZipWriteError {
    /// I/O error creating, writing, syncing, or closing the temp file.
    /// The temp file is removed before this error is returned.
    #[error("io error writing zip at {path}: {source}")]
    Io {
        /// Path to the temp file that failed.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// An entry name violates the determinism / safety contract.
    #[error("invalid entry name {name:?}: {reason}")]
    InvalidEntry {
        /// Offending archive path.
        name: String,
        /// Static reason describing the violation.
        reason: &'static str,
    },
}

/// DOS date for `1980-01-01`: bits 0–4 = day (1), bits 5–8 = month (1),
/// bits 9–15 = year-1980 (0). `(0 << 9) | (1 << 5) | 1 = 0x0021`.
const DOS_DATE_1980_01_01: u16 = 0x0021;
/// DOS time for `00:00:00`: all zero bits.
const DOS_TIME_MIDNIGHT: u16 = 0x0000;

/// `version needed to extract` — `2.0` means "Stored or DEFLATE, no Zip64".
/// We only emit Stored, but `2.0` is the canonical minimum that every reader
/// accepts.
const VERSION_NEEDED: u16 = 20;
/// `version made by` — high byte 0 = MS-DOS / FAT, low byte = `2.0`.
const VERSION_MADE_BY: u16 = 20;
/// Compression method `0` = Stored.
const METHOD_STORED: u16 = 0;
/// General-purpose bit flag: 0 (no encryption, no streamed sizes, no UTF-8
/// EFS bit). Names are pure ASCII in the snapshot file set, so bit 11 is
/// not required. Property tests pin this to keep the determinism contract
/// stable.
const GENERAL_PURPOSE_FLAG: u16 = 0;

/// Local File Header signature (`PK\x03\x04`).
const LFH_SIG: u32 = 0x0403_4b50;
/// Central Directory File Header signature (`PK\x01\x02`).
const CDFH_SIG: u32 = 0x0201_4b50;
/// End of Central Directory record signature (`PK\x05\x06`).
const EOCD_SIG: u32 = 0x0605_4b50;

/// Write `entries` to a deterministic zip archive at `dest_tmp`.
///
/// On success the file at `dest_tmp` is fully flushed and `fsync`'d. The
/// caller is responsible for renaming it to the final `.zip` destination
/// (task 6.7 performs that atomic rename).
///
/// On any error — invalid entry name, I/O failure during create / write /
/// sync, or any partial write — the partial `dest_tmp` is removed before
/// returning.
///
/// Entries are sorted by `name` lexicographically (byte-wise) before
/// writing, so two calls with the same logical entry set produce
/// byte-identical archive bytes regardless of the input order.
///
/// _Requirements: 7.1_
pub fn write_deterministic_zip(
    entries: &[ZipEntry],
    dest_tmp: &Path,
) -> Result<(), ZipWriteError> {
    // Validate every entry name *before* we touch the filesystem so
    // refusal-gate failures never produce a partial `.tmp` on disk
    // (Requirement 7.7 / 8.4 spirit, applied at the writer layer).
    for entry in entries {
        validate_entry_name(&entry.name)?;
    }

    // Sort by archive name (lexicographic on raw UTF-8 bytes). `String`'s
    // default ordering is byte-wise, which matches what readers see in the
    // central directory and makes the byte output of the writer fully
    // determined by the entry set.
    let mut sorted: Vec<&ZipEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));

    // Encode the full archive in memory before opening the file. The
    // snapshot pipeline already enforces the 50 MB payload ceiling
    // (Requirement 7.6 / 7.7) before calling here, so the in-memory buffer
    // is bounded. This also lets us avoid leaking a partial file when an
    // entry far down the list is invalid (already prevented above).
    let archive_bytes = encode_archive(&sorted);

    // Now write to disk. Any I/O error from this point on must remove the
    // temp file before returning. `inspect_err` runs the cleanup without
    // moving the error.
    write_and_fsync(dest_tmp, &archive_bytes).inspect_err(|_err| {
        // Best-effort cleanup. We deliberately ignore the result of the
        // remove_file call — the original error is the one the caller
        // needs to see.
        let _ = fs::remove_file(dest_tmp);
    })
}

fn write_and_fsync(dest_tmp: &Path, archive_bytes: &[u8]) -> Result<(), ZipWriteError> {
    let path_for_err = || -> String { dest_tmp.display().to_string() };
    let io_err = |source: std::io::Error| ZipWriteError::Io {
        path: path_for_err(),
        source,
    };

    let mut file = File::create(dest_tmp).map_err(io_err)?;
    file.write_all(archive_bytes).map_err(io_err)?;
    file.flush().map_err(io_err)?;
    file.sync_all().map_err(io_err)?;
    // Drop the handle explicitly so any close-time error surfaces. On Unix
    // close() can return errors deferred from earlier writes; we already
    // sync_all'd, but this keeps the contract explicit.
    drop(file);
    Ok(())
}

/// Validate one entry name against the determinism + safety contract.
///
/// Rejection reasons:
/// - empty,
/// - starts with `/` (absolute archive path),
/// - contains a backslash (Windows separator — names must be forward-slash
///   only per ZIP appendix and the design's portability requirement),
/// - contains a `..` path component (defense in depth against directory
///   traversal in archive consumers),
/// - contains an embedded NUL byte (would terminate names on POSIX
///   readers).
fn validate_entry_name(name: &str) -> Result<(), ZipWriteError> {
    let invalid = |reason: &'static str| ZipWriteError::InvalidEntry {
        name: name.to_owned(),
        reason,
    };

    if name.is_empty() {
        return Err(invalid("entry name is empty"));
    }
    if name.starts_with('/') {
        return Err(invalid("entry name must not start with '/'"));
    }
    if name.contains('\\') {
        return Err(invalid(
            "entry name must use forward-slash separators only",
        ));
    }
    if name.as_bytes().contains(&0) {
        return Err(invalid("entry name must not contain a NUL byte"));
    }
    for component in name.split('/') {
        if component == ".." {
            return Err(invalid(
                "entry name must not contain a '..' path component",
            ));
        }
    }
    // Name length is bounded by ZIP's u16 name-length field. We refuse
    // anything that wouldn't fit in the local file header so we never
    // silently truncate.
    if name.len() > u16::MAX as usize {
        return Err(invalid("entry name longer than 65535 bytes"));
    }
    Ok(())
}

/// Build the in-memory ZIP archive bytes for already-validated, already-sorted
/// entries.
fn encode_archive(entries: &[&ZipEntry]) -> Vec<u8> {
    // Pre-size: rough upper bound. 30 bytes LFH + name + payload per entry,
    // 46 bytes CDFH + name per entry, 22 bytes EOCD. Avoid reallocs in the
    // common case.
    let approx = entries
        .iter()
        .map(|e| 30 + 46 + e.name.len() * 2 + e.bytes.len())
        .sum::<usize>()
        + 22;
    let mut out = Vec::with_capacity(approx);

    // Per-entry CDFH descriptors built while we emit local headers; we need
    // the local-header offset and the CRC32 + size to write the central
    // directory afterwards.
    struct CdEntry<'a> {
        name: &'a str,
        crc32: u32,
        size: u32,
        local_header_offset: u32,
    }
    let mut cd_entries: Vec<CdEntry<'_>> = Vec::with_capacity(entries.len());

    for entry in entries {
        let local_header_offset = u32::try_from(out.len()).unwrap_or(u32::MAX);
        let crc = crc32_ieee(&entry.bytes);
        // ZIP local-header `compressed_size` and `uncompressed_size` are
        // u32. Stored compression ⇒ they are equal. We do not emit Zip64,
        // so the snapshot pipeline's 50 MB payload ceiling
        // (Requirement 7.6) keeps us comfortably below the u32 limit; we
        // saturate defensively rather than panic.
        let size = u32::try_from(entry.bytes.len()).unwrap_or(u32::MAX);
        let name_bytes = entry.name.as_bytes();
        let name_len = u16::try_from(name_bytes.len()).unwrap_or(u16::MAX);

        // Local File Header.
        write_u32_le(&mut out, LFH_SIG);
        write_u16_le(&mut out, VERSION_NEEDED);
        write_u16_le(&mut out, GENERAL_PURPOSE_FLAG);
        write_u16_le(&mut out, METHOD_STORED);
        write_u16_le(&mut out, DOS_TIME_MIDNIGHT);
        write_u16_le(&mut out, DOS_DATE_1980_01_01);
        write_u32_le(&mut out, crc);
        write_u32_le(&mut out, size); // compressed_size (Stored ⇒ == size)
        write_u32_le(&mut out, size); // uncompressed_size
        write_u16_le(&mut out, name_len);
        write_u16_le(&mut out, 0); // extra field length
        out.extend_from_slice(name_bytes);
        // Stored data is the payload bytes verbatim.
        out.extend_from_slice(&entry.bytes);

        cd_entries.push(CdEntry {
            name: entry.name.as_str(),
            crc32: crc,
            size,
            local_header_offset,
        });
    }

    // Central Directory.
    let cd_offset = u32::try_from(out.len()).unwrap_or(u32::MAX);
    for cd in &cd_entries {
        let name_bytes = cd.name.as_bytes();
        let name_len = u16::try_from(name_bytes.len()).unwrap_or(u16::MAX);

        write_u32_le(&mut out, CDFH_SIG);
        write_u16_le(&mut out, VERSION_MADE_BY);
        write_u16_le(&mut out, VERSION_NEEDED);
        write_u16_le(&mut out, GENERAL_PURPOSE_FLAG);
        write_u16_le(&mut out, METHOD_STORED);
        write_u16_le(&mut out, DOS_TIME_MIDNIGHT);
        write_u16_le(&mut out, DOS_DATE_1980_01_01);
        write_u32_le(&mut out, cd.crc32);
        write_u32_le(&mut out, cd.size); // compressed_size
        write_u32_le(&mut out, cd.size); // uncompressed_size
        write_u16_le(&mut out, name_len);
        write_u16_le(&mut out, 0); // extra field length
        write_u16_le(&mut out, 0); // file comment length
        write_u16_le(&mut out, 0); // disk number start
        write_u16_le(&mut out, 0); // internal file attributes
        write_u32_le(&mut out, 0); // external file attributes (0 = portable)
        write_u32_le(&mut out, cd.local_header_offset);
        out.extend_from_slice(name_bytes);
    }
    let cd_size = u32::try_from(out.len())
        .unwrap_or(u32::MAX)
        .saturating_sub(cd_offset);

    // End of Central Directory.
    let entry_count = u16::try_from(cd_entries.len()).unwrap_or(u16::MAX);
    write_u32_le(&mut out, EOCD_SIG);
    write_u16_le(&mut out, 0); // disk number
    write_u16_le(&mut out, 0); // disk where CD starts
    write_u16_le(&mut out, entry_count); // entries on this disk
    write_u16_le(&mut out, entry_count); // total entries
    write_u32_le(&mut out, cd_size);
    write_u32_le(&mut out, cd_offset);
    write_u16_le(&mut out, 0); // .zip comment length

    out
}

#[inline]
fn write_u16_le(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

#[inline]
fn write_u32_le(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// CRC32 with the IEEE 802.3 polynomial (reflected `0xedb88320`), as used
/// by ZIP. Hand-rolled byte-at-a-time table-driven implementation; keeps
/// the snapshot pipeline free of new dependencies. The table is built at
/// runtime once per call which is fine — snapshot exports are bounded at
/// 50 MB and not on a hot loop.
fn crc32_ieee(bytes: &[u8]) -> u32 {
    let table = crc32_table();
    let mut crc: u32 = 0xffff_ffff;
    for &b in bytes {
        let idx = ((crc ^ u32::from(b)) & 0xff) as usize;
        crc = (crc >> 8) ^ table[idx];
    }
    crc ^ 0xffff_ffff
}

fn crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0u32;
    while i < 256 {
        let mut c = i;
        let mut j = 0;
        while j < 8 {
            c = if c & 1 == 1 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            j += 1;
        }
        table[i as usize] = c;
        i += 1;
    }
    table
}

// Re-export the temp-path helper type for tests / callers that want to
// describe "where the .tmp file should be" without leaking `PathBuf`
// allocation responsibility into call sites.
#[allow(dead_code)]
pub(crate) type TmpPath = PathBuf;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn temp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        // Include a random-ish nonce so parallel test runs don't collide.
        // We use the test name + a process id; no randomness is consulted
        // in production code — this is test-only scaffolding.
        p.push(format!(
            "stats-code-zip-writer-{}-{}.tmp",
            name,
            std::process::id()
        ));
        // Best-effort cleanup of any leftover from a previous run.
        let _ = fs::remove_file(&p);
        p
    }

    fn entry(name: &str, bytes: &[u8]) -> ZipEntry {
        ZipEntry {
            name: name.to_owned(),
            bytes: bytes.to_vec(),
        }
    }

    fn read_file(path: &Path) -> Vec<u8> {
        let mut f = File::open(path).expect("temp file exists after write");
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).expect("temp file readable");
        buf
    }

    #[test]
    fn happy_path_two_entries_produces_valid_zip_magic() {
        let dest = temp_path("happy");
        let entries = vec![entry("a.txt", b"hello"), entry("b.txt", b"world!")];
        write_deterministic_zip(&entries, &dest).expect("write succeeds");
        let bytes = read_file(&dest);
        // First four bytes must be the local file header signature.
        assert_eq!(&bytes[0..4], b"PK\x03\x04");
        // Last 22+ bytes contain EOCD; signature appears at len-22 when
        // there is no archive comment (we emit none).
        let len = bytes.len();
        assert!(len >= 22, "archive too small: {len} bytes");
        assert_eq!(&bytes[len - 22..len - 18], b"PK\x05\x06");
        let _ = fs::remove_file(&dest);
    }

    #[test]
    fn output_is_byte_for_byte_deterministic() {
        let dest1 = temp_path("det1");
        let dest2 = temp_path("det2");
        let entries = vec![
            entry("a.txt", b"hello"),
            entry("dir/b.bin", &[0u8, 1, 2, 3, 0xff, 0x80]),
            entry("z.md", b"# title\n"),
        ];
        write_deterministic_zip(&entries, &dest1).unwrap();
        write_deterministic_zip(&entries, &dest2).unwrap();
        let b1 = read_file(&dest1);
        let b2 = read_file(&dest2);
        assert_eq!(b1, b2, "two writes of same entries must be byte-identical");
        let _ = fs::remove_file(&dest1);
        let _ = fs::remove_file(&dest2);
    }

    #[test]
    fn entries_are_sorted_lexicographically_before_writing() {
        let dest_sorted = temp_path("sorted");
        let dest_reverse = temp_path("reverse");
        let a = entry("a.txt", b"AAA");
        let b = entry("b.txt", b"BBB");
        let c = entry("c.txt", b"CCC");
        write_deterministic_zip(
            &[a.clone(), b.clone(), c.clone()],
            &dest_sorted,
        )
        .unwrap();
        write_deterministic_zip(&[c, b, a], &dest_reverse).unwrap();
        let bs = read_file(&dest_sorted);
        let br = read_file(&dest_reverse);
        assert_eq!(
            bs, br,
            "sorted-input and reversed-input writes must produce identical bytes"
        );
        // Sanity: payload "AAA" appears before "BBB", which appears before
        // "CCC", in the archive bytes.
        let pos_a = bs.windows(3).position(|w| w == b"AAA").expect("AAA");
        let pos_b = bs.windows(3).position(|w| w == b"BBB").expect("BBB");
        let pos_c = bs.windows(3).position(|w| w == b"CCC").expect("CCC");
        assert!(pos_a < pos_b && pos_b < pos_c);
        let _ = fs::remove_file(&dest_sorted);
        let _ = fs::remove_file(&dest_reverse);
    }

    #[test]
    fn empty_entry_set_writes_a_valid_empty_archive() {
        let dest = temp_path("empty");
        write_deterministic_zip(&[], &dest).unwrap();
        let bytes = read_file(&dest);
        // Empty archive == EOCD only, 22 bytes.
        assert_eq!(bytes.len(), 22);
        assert_eq!(&bytes[0..4], b"PK\x05\x06");
        // entry counts and CD size are zero.
        for offset in [8, 10, 12, 16] {
            assert_eq!(&bytes[offset..offset + 2], &[0, 0][..]);
        }
        let _ = fs::remove_file(&dest);
    }

    #[test]
    fn fixed_mtime_dos_date_is_1980_01_01_midnight() {
        let dest = temp_path("mtime");
        write_deterministic_zip(&[entry("x", b"x")], &dest).unwrap();
        let bytes = read_file(&dest);
        // Local file header layout: sig(4) ver(2) flag(2) method(2)
        // mod_time(2) mod_date(2) ...
        let mod_time = u16::from_le_bytes([bytes[10], bytes[11]]);
        let mod_date = u16::from_le_bytes([bytes[12], bytes[13]]);
        assert_eq!(mod_time, DOS_TIME_MIDNIGHT);
        assert_eq!(mod_date, DOS_DATE_1980_01_01);
        let _ = fs::remove_file(&dest);
    }

    #[test]
    fn compression_method_is_stored() {
        let dest = temp_path("stored");
        // Highly compressible payload — under DEFLATE the stored bytes
        // would shrink. Under Stored they are copied verbatim, so the
        // payload must appear in the output.
        let payload = vec![b'A'; 1024];
        write_deterministic_zip(&[entry("x", &payload)], &dest).unwrap();
        let bytes = read_file(&dest);
        // Method field in LFH is at offset 8 (after sig+ver+flag).
        let method = u16::from_le_bytes([bytes[8], bytes[9]]);
        assert_eq!(method, METHOD_STORED);
        // Verbatim payload is present.
        assert!(
            bytes.windows(payload.len()).any(|w| w == payload.as_slice()),
            "stored payload must appear verbatim in the archive bytes"
        );
        let _ = fs::remove_file(&dest);
    }

    #[test]
    fn rejects_empty_entry_name() {
        let dest = temp_path("name-empty");
        let err = write_deterministic_zip(&[entry("", b"x")], &dest).unwrap_err();
        match err {
            ZipWriteError::InvalidEntry { name, .. } => assert_eq!(name, ""),
            ZipWriteError::Io { .. } => panic!("expected InvalidEntry, got Io"),
        }
        // Validation happens before file creation, so no temp file exists.
        assert!(!dest.exists());
    }

    #[test]
    fn rejects_leading_slash_entry_name() {
        let dest = temp_path("name-leading-slash");
        let err = write_deterministic_zip(&[entry("/abs", b"x")], &dest).unwrap_err();
        assert!(matches!(err, ZipWriteError::InvalidEntry { .. }));
        assert!(!dest.exists());
    }

    #[test]
    fn rejects_dotdot_path_component() {
        let dest = temp_path("name-dotdot");
        let err =
            write_deterministic_zip(&[entry("a/../b", b"x")], &dest).unwrap_err();
        assert!(matches!(err, ZipWriteError::InvalidEntry { .. }));
        assert!(!dest.exists());
    }

    #[test]
    fn rejects_backslash_in_entry_name() {
        let dest = temp_path("name-backslash");
        let err =
            write_deterministic_zip(&[entry("a\\b", b"x")], &dest).unwrap_err();
        assert!(matches!(err, ZipWriteError::InvalidEntry { .. }));
        assert!(!dest.exists());
    }

    #[test]
    fn rejects_nul_in_entry_name() {
        let dest = temp_path("name-nul");
        let err = write_deterministic_zip(&[entry("a\0b", b"x")], &dest).unwrap_err();
        assert!(matches!(err, ZipWriteError::InvalidEntry { .. }));
        assert!(!dest.exists());
    }

    #[test]
    fn invalid_entry_after_valid_entry_does_not_create_file() {
        let dest = temp_path("name-mixed");
        let err = write_deterministic_zip(
            &[entry("ok.txt", b"hello"), entry("/bad", b"x")],
            &dest,
        )
        .unwrap_err();
        assert!(matches!(err, ZipWriteError::InvalidEntry { .. }));
        // No partial .tmp must exist.
        assert!(
            !dest.exists(),
            "invalid entry must not leave a partial temp file"
        );
    }

    #[test]
    fn io_error_on_unwritable_dest_removes_partial_file() {
        // Use a path whose parent directory does not exist — File::create
        // will fail before any bytes are written. We assert that the path
        // does not exist after the call regardless.
        let mut dest = std::env::temp_dir();
        dest.push("stats-code-zip-writer-nonexistent-parent");
        dest.push("nested");
        dest.push("missing.tmp");
        let err =
            write_deterministic_zip(&[entry("a", b"x")], &dest).unwrap_err();
        assert!(matches!(err, ZipWriteError::Io { .. }));
        assert!(!dest.exists());
    }

    #[test]
    fn crc32_matches_known_vectors() {
        // Standard test vectors for IEEE CRC32.
        // "" -> 0
        assert_eq!(crc32_ieee(b""), 0);
        // "a" -> 0xe8b7be43
        assert_eq!(crc32_ieee(b"a"), 0xe8b7_be43);
        // "abc" -> 0x352441c2
        assert_eq!(crc32_ieee(b"abc"), 0x3524_41c2);
        // "123456789" -> 0xcbf43926 (canonical CRC-32 check value)
        assert_eq!(crc32_ieee(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn entry_payload_crc_round_trip_through_archive_bytes() {
        let dest = temp_path("crc");
        let payload = b"the quick brown fox jumps over the lazy dog";
        write_deterministic_zip(&[entry("fox", payload)], &dest).unwrap();
        let bytes = read_file(&dest);
        // Local-header CRC is at offset 14 (after sig 4 + ver 2 + flag 2 +
        // method 2 + mtime 2 + mdate 2).
        let crc_in_zip = u32::from_le_bytes([
            bytes[14], bytes[15], bytes[16], bytes[17],
        ]);
        assert_eq!(crc_in_zip, crc32_ieee(payload));
        let _ = fs::remove_file(&dest);
    }

    #[test]
    fn name_length_in_header_matches_actual_name() {
        let dest = temp_path("namelen");
        let name = "some/path/inside.txt";
        write_deterministic_zip(&[entry(name, b"data")], &dest).unwrap();
        let bytes = read_file(&dest);
        // file_name length field is at offset 26 in LFH.
        let name_len = u16::from_le_bytes([bytes[26], bytes[27]]);
        assert_eq!(name_len as usize, name.len());
        // extra field length is at offset 28, must be zero (we emit no
        // extras — keeps determinism trivial).
        let extra_len = u16::from_le_bytes([bytes[28], bytes[29]]);
        assert_eq!(extra_len, 0);
        let _ = fs::remove_file(&dest);
    }
}
