//! Property test: Snippet header always carries column metadata, dataset
//! SHA256, and release version.
//!
//! **Validates: Requirements 1.7, 2.5**
//!
//! Property 3 from the parity-and-multilang-sidecar design: proptest generates
//! arbitrary columns / sha256 / release_version combinations and asserts the
//! formatted header contains `data.csv`, every column name, every dtype token,
//! the 64-hex lowercase sha256, and the release version string.

use proptest::prelude::*;
use stats_code::sidecar::header::{format_header, Column, ColumnDtype};

/// Strategy for generating a valid ColumnDtype.
fn arb_dtype() -> impl Strategy<Value = ColumnDtype> {
    prop_oneof![
        Just(ColumnDtype::Numeric),
        Just(ColumnDtype::Categorical),
        Just(ColumnDtype::Date),
        Just(ColumnDtype::String),
    ]
}

/// Strategy for generating a non-empty column name.
/// Uses printable ASCII excluding `\n` and `\r` to keep assertions simple.
fn arb_column_name() -> impl Strategy<Value = String> {
    "[a-zA-Z_][a-zA-Z0-9_]{0,30}".prop_map(|s| s)
}

/// Strategy for generating a single Column.
fn arb_column() -> impl Strategy<Value = Column> {
    (arb_column_name(), arb_dtype()).prop_map(|(name, dtype)| Column { name, dtype })
}

/// Strategy for generating a valid 64-character lowercase hex SHA256 string.
fn arb_sha256() -> impl Strategy<Value = String> {
    proptest::collection::vec(prop_oneof![0u8..=9, 10u8..=15], 32).prop_map(|bytes| {
        bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    })
}

/// Strategy for generating a non-empty release version string.
fn arb_release_version() -> impl Strategy<Value = String> {
    "[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}(-[a-z0-9.]{1,10})?"
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, .. ProptestConfig::default() })]

    /// **Validates: Requirements 1.7, 2.5**
    ///
    /// Property 3: The formatted header always contains:
    /// - the literal "data.csv"
    /// - every column name
    /// - every dtype token
    /// - the 64-hex lowercase sha256
    /// - the release version string
    #[test]
    fn header_carries_column_metadata_sha256_and_version(
        columns in proptest::collection::vec(arb_column(), 0..8),
        sha256 in arb_sha256(),
        version in arb_release_version(),
    ) {
        let header = format_header(&columns, &sha256, &version);

        // 1. Must contain the literal "data.csv"
        prop_assert!(
            header.contains("data.csv"),
            "header must contain the literal 'data.csv', got:\n{header}"
        );

        // 2. Must contain every column name
        for (i, col) in columns.iter().enumerate() {
            prop_assert!(
                header.contains(&col.name),
                "header must contain column name '{}' (index {i}), got:\n{header}",
                col.name
            );
        }

        // 3. Must contain every dtype token
        for (i, col) in columns.iter().enumerate() {
            prop_assert!(
                header.contains(col.dtype.as_token()),
                "header must contain dtype token '{}' for column index {i}, got:\n{header}",
                col.dtype.as_token()
            );
        }

        // 4. Must contain the 64-hex lowercase sha256
        prop_assert!(
            header.contains(&sha256),
            "header must contain the dataset sha256 '{sha256}', got:\n{header}"
        );
        // Verify the sha256 in the header is exactly 64 lowercase hex chars
        prop_assert_eq!(sha256.len(), 64);
        prop_assert!(sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));

        // 5. Must contain the release version
        prop_assert!(
            header.contains(&version),
            "header must contain the release version '{version}', got:\n{header}"
        );

        // Bonus structural invariants from Requirement 2.1:
        // - LF-only (no CR)
        prop_assert!(
            !header.contains('\r'),
            "header must use LF-only line endings (no CR)"
        );
        // - Ends with LF
        prop_assert!(
            header.ends_with('\n'),
            "header must end with a trailing LF"
        );
    }
}
