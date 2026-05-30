//! Property 4: Redaction soundness across every emitted artifact.
//!
//! **Validates: Requirements 2.6, 9.1, 9.4, 9.5**
//!
//! Two properties are tested:
//!
//! 1. For any non-empty API key K and any input text containing K, the output
//!    of `redact_pure` never contains K as a substring.
//! 2. For any absolute path outside the working directory, the output field
//!    for that path is exactly `<external>`.

use proptest::prelude::*;
use stats_code::redact::{redact_pure, RedactionPolicy};

/// Strategy: generate a non-empty secret string (1–64 chars) drawn from the
/// alphabet `[A-Z0-9]`.
///
/// The alphabet is deliberately **disjoint** from both the redaction marker
/// `<redacted>` (all lowercase ASCII plus angle brackets) and the lowercase
/// filler used by the leak properties below. This disjointness is what makes
/// the `!output.contains(secret)` soundness assertion valid even when proptest
/// shrinks the secret to a single character: an uppercase/digit secret can
/// never appear inside the lowercase marker word `redacted` (the bug that
/// would otherwise flag a correctly-redacted `"prefix <redacted> suffix"` as a
/// leak just because the marker contains the letter `a`), nor inside the
/// lowercase scaffolding. Restricting the generator alphabet does not weaken
/// the production guarantee — `redact_pure` redacts any exact-substring secret
/// regardless of format; it only removes false positives from the test oracle.
fn arb_secret() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[A-Z0-9]{1,64}").expect("valid secret regex")
}

/// Strategy: generate an absolute path that is clearly outside any plausible
/// working directory. We use Unix-style `/home/<user>/...` and Windows-style
/// `C:\Users\<user>\...` paths.
fn arb_external_path() -> impl Strategy<Value = String> {
    prop_oneof![
        // Unix external path
        "[a-z]{3,12}".prop_map(|user| format!("/home/{user}/documents/secret.csv")),
        // Windows external path
        "[a-z]{3,12}".prop_map(|user| format!("C:\\Users\\{user}\\Desktop\\file.xlsx")),
        // macOS external path
        "[a-z]{3,12}".prop_map(|user| format!("/Users/{user}/Library/data.json")),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    /// Property 4a: After redaction, the output byte stream never contains
    /// the secret K as a substring.
    ///
    /// **Validates: Requirements 2.6, 9.1, 9.4, 9.5**
    #[test]
    fn secret_never_appears_in_redacted_output(
        secret in arb_secret(),
    ) {
        // Build text that definitely contains the secret.
        let runner = proptest::test_runner::TestRunner::default();
        let _ = runner;

        // Generate text containing the secret by embedding it.
        let text = format!("prefix {} suffix", &secret);
        let policy = RedactionPolicy::new().with_secrets(&[secret.as_str()]);
        let output = redact_pure(&text, &policy);

        prop_assert!(
            !output.contains(&secret),
            "Secret {:?} leaked into output: {:?}",
            secret,
            output
        );
    }

    /// Property 4b: Secret embedded at arbitrary positions in arbitrary text
    /// is always fully removed from the output.
    ///
    /// **Validates: Requirements 2.6, 9.1, 9.4**
    #[test]
    fn secret_in_arbitrary_context_never_leaks(
        secret in arb_secret(),
        prefix in "[a-z _,;:]{0,64}",
        suffix in "[a-z _,;:]{0,64}",
        middle in "[a-z _,;:]{0,32}",
    ) {
        // Place the secret multiple times in the text.
        let text = format!("{prefix}{secret}{middle}{secret}{suffix}");
        let policy = RedactionPolicy::new().with_secrets(&[secret.as_str()]);
        let output = redact_pure(&text, &policy);

        prop_assert!(
            !output.contains(&secret),
            "Secret {:?} leaked into output: {:?}",
            secret,
            output
        );
    }

    /// Property 4c: Any absolute path outside the working directory is
    /// replaced with `<external>` in the output.
    ///
    /// **Validates: Requirements 9.5**
    #[test]
    fn external_path_becomes_external_marker(
        ext_path in arb_external_path(),
        prefix in "[a-zA-Z0-9 =]{0,32}",
        suffix in "[a-zA-Z0-9 =]{0,32}",
    ) {
        // Use a working directory that is guaranteed to be disjoint from
        // the generated external paths.
        let policy = RedactionPolicy::new()
            .with_working_directory(std::path::PathBuf::from("/opt/analysis_workspace"));

        let text = format!("{prefix} {ext_path} {suffix}");
        let output = redact_pure(&text, &policy);

        // The raw external path must not appear in the output.
        prop_assert!(
            !output.contains(&ext_path),
            "External path {:?} leaked into output: {:?}",
            ext_path,
            output
        );

        // The `<external>` sentinel must be present (the path was replaced).
        prop_assert!(
            output.contains("<external>"),
            "Expected <external> marker in output: {:?}",
            output
        );
    }

    /// Property 4d: Paths outside the working directory on Windows are also
    /// replaced with `<external>`.
    ///
    /// **Validates: Requirements 9.5**
    #[test]
    fn windows_external_path_becomes_external_marker(
        user in "[a-z]{3,12}",
        filename in "[a-z]{1,8}",
    ) {
        // Working directory is on a different drive / different subtree.
        let policy = RedactionPolicy::new()
            .with_working_directory(std::path::PathBuf::from(r"D:\my_project"));

        let ext_path = format!("C:\\Users\\{user}\\{filename}.csv");
        let text = format!("loaded {ext_path}");
        let output = redact_pure(&text, &policy);

        prop_assert!(
            !output.contains(&format!("C:\\Users\\{user}")),
            "Windows external path leaked: {:?}",
            output
        );
        prop_assert!(
            output.contains("<external>"),
            "Expected <external> marker in output: {:?}",
            output
        );
    }
}
