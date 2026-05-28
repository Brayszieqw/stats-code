//! Sidecar snippet header builder.
//!
//! Feature: parity-and-multilang-sidecar — task 2.2.
//!
//! `format_header` is the pure function that prepends every emitted
//! Sidecar snippet with a language-agnostic comment block listing
//!
//! 1. an identifying banner,
//! 2. the Stats Code release version,
//! 3. the input dataset's SHA256 (64-character lowercase hex),
//! 4. the literal `data.csv` reference required by Requirement 1.7 / 2.5,
//! 5. the input column metadata (name + dtype) in caller-supplied order.
//!
//! The function is referentially transparent: it reads no clock, no
//! environment variables, no random source, and performs no I/O. All
//! output is UTF-8 with LF line endings, no BOM, never `\r` — this is the
//! determinism contract carried into Requirement 2.1 by the upstream
//! `generate_snippet` orchestration.
//!
//! SHA256 invariants are caller-supplied (every call site in this crate
//! runs the bytes through a vetted hasher first), so they are encoded as
//! `debug_assert*!` rather than runtime panics: dev / test builds catch
//! misuse loudly, release builds keep the snippet path branch-free.

use std::fmt::{self, Write as _};

/// Column data type, projected onto a stable lowercase token used by every
/// host language's sidecar template.
///
/// The four variants match the closed set defined in Requirement 2.5
/// (`numeric | categorical | date | string`). `as_token` returns a
/// `&'static str` so the renderer never allocates while emitting headers.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ColumnDtype {
    Numeric,
    Categorical,
    Date,
    String,
}

impl ColumnDtype {
    /// Stable lowercase token for the dtype, exactly as it appears in the
    /// emitted header line (`# column.<i>.dtype: <token>`).
    #[must_use]
    pub const fn as_token(self) -> &'static str {
        match self {
            ColumnDtype::Numeric => "numeric",
            ColumnDtype::Categorical => "categorical",
            ColumnDtype::Date => "date",
            ColumnDtype::String => "string",
        }
    }
}

impl fmt::Display for ColumnDtype {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_token())
    }
}

/// One input column's name + dtype as carried into the sidecar header.
///
/// This is the minimal surface needed by the header builder; the wider
/// renderer (task 2.3) and `generate_snippet` (task 2.5) will reuse the
/// same struct so column metadata never round-trips through a stringly
/// typed map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Column {
    pub name: String,
    pub dtype: ColumnDtype,
}

/// Format the language-agnostic comment header that prefaces every
/// Sidecar snippet.
///
/// # Output shape
///
/// ```text
/// # stats-code sidecar snippet
/// # release: <release_version>
/// # dataset_sha256: <dataset_sha256>
/// # data: data.csv
/// # column.0.name: <columns[0].name>
/// # column.0.dtype: <columns[0].dtype.as_token()>
/// # column.1.name: <columns[1].name>
/// # column.1.dtype: <columns[1].dtype.as_token()>
/// ...
/// ```
///
/// Every line is `\n`-terminated (never `\r\n`), the result is valid
/// UTF-8 with no BOM, and column iteration follows the input slice order
/// — never sorted.
///
/// # Caller invariants (debug-asserted)
///
/// * `dataset_sha256.len() == 64`
/// * Every byte of `dataset_sha256` is in `[0-9a-f]` (ASCII lowercase hex)
///
/// Both invariants are upheld by every call site in this crate: hashes are
/// produced by a vetted `Sha256` instance and rendered with the standard
/// lowercase hex formatter before reaching this function. Encoding them as
/// `debug_assert*!` keeps the release path allocation-only and surfaces
/// misuse loudly during test runs.
#[must_use]
pub fn format_header(
    columns: &[Column],
    dataset_sha256: &str,
    release_version: &str,
) -> String {
    debug_assert_eq!(
        dataset_sha256.len(),
        64,
        "dataset_sha256 must be a 64-character hex string"
    );
    debug_assert!(
        dataset_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f')),
        "dataset_sha256 must be lowercase hex"
    );

    // Reserve a generous buffer to avoid reallocations on the typical
    // 4-to-20 column case. Each column contributes ~60 bytes.
    let mut out = String::with_capacity(160 + columns.len() * 64);

    // `writeln!` always emits `\n`, regardless of host platform — that is
    // the LF guarantee Requirement 2.1 leans on. Writes to `String` are
    // infallible; the `expect` documents the invariant for readers.
    writeln!(out, "# stats-code sidecar snippet").expect("write to String is infallible");
    writeln!(out, "# release: {release_version}").expect("write to String is infallible");
    writeln!(out, "# dataset_sha256: {dataset_sha256}").expect("write to String is infallible");
    writeln!(out, "# data: data.csv").expect("write to String is infallible");

    for (i, column) in columns.iter().enumerate() {
        writeln!(out, "# column.{i}.name: {}", column.name)
            .expect("write to String is infallible");
        writeln!(out, "# column.{i}.dtype: {}", column.dtype.as_token())
            .expect("write to String is infallible");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canonical lowercase 64-hex fixture used across every header test
    /// that does not explicitly want to exercise an invariant violation.
    const SHA256_LOWER: &str =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn empty_columns_still_produces_complete_header() {
        let header = format_header(&[], SHA256_LOWER, "0.5.0");

        assert_eq!(
            header,
            concat!(
                "# stats-code sidecar snippet\n",
                "# release: 0.5.0\n",
                "# dataset_sha256: \
                 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n",
                "# data: data.csv\n",
            ),
        );
    }

    #[test]
    fn multi_column_input_preserves_order_and_renders_lowercase_dtypes() {
        let columns = vec![
            Column {
                name: "age".into(),
                dtype: ColumnDtype::Numeric,
            },
            Column {
                name: "sex".into(),
                dtype: ColumnDtype::Categorical,
            },
            Column {
                name: "date".into(),
                dtype: ColumnDtype::Date,
            },
            Column {
                name: "site".into(),
                dtype: ColumnDtype::String,
            },
        ];
        let header = format_header(&columns, SHA256_LOWER, "0.5.0");

        assert_eq!(
            header,
            concat!(
                "# stats-code sidecar snippet\n",
                "# release: 0.5.0\n",
                "# dataset_sha256: \
                 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n",
                "# data: data.csv\n",
                "# column.0.name: age\n",
                "# column.0.dtype: numeric\n",
                "# column.1.name: sex\n",
                "# column.1.dtype: categorical\n",
                "# column.2.name: date\n",
                "# column.2.dtype: date\n",
                "# column.3.name: site\n",
                "# column.3.dtype: string\n",
            ),
        );
    }

    #[test]
    fn output_contains_data_csv_literal_exactly_once() {
        let columns = vec![Column {
            name: "x".into(),
            dtype: ColumnDtype::Numeric,
        }];
        let header = format_header(&columns, SHA256_LOWER, "1.2.3");
        assert_eq!(
            header.matches("data.csv").count(),
            1,
            "header must mention data.csv exactly once"
        );
    }

    #[test]
    fn output_uses_lf_only_and_ends_with_single_trailing_lf() {
        let columns = vec![
            Column {
                name: "alpha".into(),
                dtype: ColumnDtype::Numeric,
            },
            Column {
                name: "beta".into(),
                dtype: ColumnDtype::Categorical,
            },
        ];
        let header = format_header(&columns, SHA256_LOWER, "9.9.9");

        assert!(!header.contains('\r'), "header must not contain CR bytes");
        assert!(header.ends_with('\n'), "header must end with LF");
        assert!(
            !header.ends_with("\n\n"),
            "header must end with exactly one trailing LF"
        );
        // Sanity: every line ends with LF (no trailing partial line).
        for line in header.split_inclusive('\n') {
            assert!(line.ends_with('\n'));
        }
    }

    #[test]
    fn all_four_dtype_tokens_render_lowercase() {
        assert_eq!(ColumnDtype::Numeric.as_token(), "numeric");
        assert_eq!(ColumnDtype::Categorical.as_token(), "categorical");
        assert_eq!(ColumnDtype::Date.as_token(), "date");
        assert_eq!(ColumnDtype::String.as_token(), "string");
    }

    #[test]
    fn dataset_sha256_appears_verbatim_in_output() {
        let sha = "deadbeef".repeat(8); // 64 lowercase hex chars exactly
        let header = format_header(&[], &sha, "0.5.0");
        assert!(header.contains(&sha), "expected verbatim sha256 in header");
        assert_eq!(header.matches(sha.as_str()).count(), 1);
    }

    #[test]
    fn release_version_appears_verbatim_in_output() {
        let header = format_header(&[], SHA256_LOWER, "1.2.3-rc.4+build.7");
        assert!(header.contains("# release: 1.2.3-rc.4+build.7\n"));
    }

    #[test]
    fn function_is_pure_and_idempotent_byte_for_byte() {
        let columns = vec![
            Column {
                name: "x".into(),
                dtype: ColumnDtype::Numeric,
            },
            Column {
                name: "y".into(),
                dtype: ColumnDtype::Date,
            },
        ];
        let a = format_header(&columns, SHA256_LOWER, "0.5.0");
        let b = format_header(&columns, SHA256_LOWER, "0.5.0");
        assert_eq!(a, b);
    }

    // --- Invariant violations: documented as `debug_assert*!`, exercised
    // only in debug builds (which `cargo test` defaults to). The
    // `#[cfg(debug_assertions)]` gates compile these tests out under
    // `cargo test --release`, where `debug_assert*!` is a no-op so the
    // function would not panic. -------------

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "must be lowercase hex")]
    fn uppercase_sha256_violates_lowercase_invariant() {
        let sha = "DEADBEEF".repeat(8); // 64 chars, but uppercase
        let _ = format_header(&[], &sha, "0.5.0");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "must be a 64-character hex string")]
    fn short_sha256_violates_length_invariant() {
        let _ = format_header(&[], "abc", "0.5.0");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "must be a 64-character hex string")]
    fn long_sha256_violates_length_invariant() {
        let sha = "a".repeat(65);
        let _ = format_header(&[], &sha, "0.5.0");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "must be lowercase hex")]
    fn non_hex_sha256_violates_alphabet_invariant() {
        let mut sha = "a".repeat(63);
        sha.push('z'); // 64 chars, last one outside [0-9a-f]
        let _ = format_header(&[], &sha, "0.5.0");
    }
}
