//! Secret-and-path redaction policy shared between the Sidecar Code Generator
//! and the Audit Snapshot Exporter.
//!
//! Feature: parity-and-multilang-sidecar — task 2.4.
//!
//! `redact_pure` is the deterministic, side-effect-free string rewriter that
//! every emitted artifact passes through before reaching disk:
//!
//! * Sidecar snippets call it with `RedactionPolicy::with_secrets(&[api_key])`
//!   and `with_working_directory(<analysis cwd>)` so the rendered code
//!   contains no provider credential and no out-of-cwd absolute path
//!   (Requirements 2.6, 9.1, 9.4, 9.5).
//! * Snapshot fields (manifest values, narrative prose, llm provenance,
//!   workflow YAML strings) call it with the same policy so the same
//!   guarantees hold across the full snapshot zip
//!   (Requirements 9.1, 9.3, 9.4, 9.5).
//!
//! # Purity contract
//!
//! `redact_pure` reads no clock, no environment variable, no random source,
//! opens no file, and acquires no lock. Two invocations on different hosts at
//! different times with structurally identical inputs return byte-identical
//! `String`s. The function is total: it cannot fail (bad UTF-8 cannot reach
//! it because the input is `&str`).
//!
//! # Algorithm
//!
//! 1. **Secret substitution.** Every secret recorded in the policy is
//!    replaced by the literal `<redacted>` everywhere it appears. Secrets
//!    are processed *longest-first* so that overlapping secrets do not mask
//!    each other (e.g. with secrets `["AB", "ABCD"]` the input `"xABCDy"`
//!    becomes `"x<redacted>y"`, never `"x<redacted>CDy"`). Empty secrets are
//!    silently dropped — an empty needle would match between every byte and
//!    corrupt the output.
//! 2. **Path classification.** Substrings that look like absolute file-system
//!    paths are detected by a small hand-rolled scanner (no regex, no
//!    dependency surface). For each detected path:
//!    * If the policy carries a working directory and the path lies inside
//!      it, the path is rewritten as the *relative* form using forward
//!      slashes regardless of host platform — that keeps emitted artifacts
//!      byte-deterministic across hosts (Requirement 2.1).
//!    * Otherwise the entire path substring is replaced by the literal
//!      `<external>`.
//!
//! Both passes run in order: secrets first, then paths.
//!
//! # Detection rule (conservative)
//!
//! The scanner only matches what looks unambiguously like a real filesystem
//! path:
//!
//! * **Windows drive-letter paths** — `[A-Za-z]:[\\/]` followed by a run of
//!   path-content bytes (ASCII alphanumerics, `\`, `/`, `.`, `_`, `-`, `~`).
//! * **Unix absolute paths** rooted at one of the explicit prefixes
//!   `/Users/`, `/home/`, `/root/`, `/private/`, `/tmp/`, `/var/`.
//!
//! Path candidates only start at a *boundary* — either at offset 0, or
//! immediately after a byte that is not itself a path-content byte. That
//! single rule is what keeps URLs like `https://example.com/home/alice`
//! from being mis-classified as filesystem paths: the bytes between `://`
//! and the trailing `/home/alice` are all path-content bytes, so the inner
//! `/home/` never sits at a boundary.
//!
//! # Idempotence
//!
//! `redact_pure(redact_pure(s, p), p) == redact_pure(s, p)` for every `s`
//! and `p`. After the first pass `<redacted>` and `<external>` are present
//! in the output, but neither matches a secret nor a path prefix, so a
//! second application is a no-op. The idempotence test in this module
//! locks that contract.

use std::path::PathBuf;

/// Policy describing how `redact_pure` should rewrite a string.
///
/// Build a policy with [`RedactionPolicy::new`] and then attach secrets and
/// (optionally) a working directory via the chainable `with_*` builders.
///
/// The policy is intentionally cheap to clone and easy to compose in tests:
/// no I/O, no statics, no global state.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct RedactionPolicy {
    /// Secrets to rewrite to `<redacted>` on every occurrence. Empty
    /// strings are filtered out by [`RedactionPolicy::with_secrets`] so an
    /// empty needle can never reach the rewriter.
    secrets: Vec<String>,
    /// Working directory used to classify detected paths as either
    /// "inside the cwd" (rewritten to a forward-slash relative form) or
    /// "outside the cwd" (rewritten to `<external>`). `None` makes every
    /// detected path `<external>`.
    working_directory: Option<PathBuf>,
}

impl RedactionPolicy {
    /// Create an empty policy. By itself this policy is a no-op: it carries
    /// no secrets and no working directory, so `redact_pure(text, &policy)`
    /// returns `text` verbatim.
    #[must_use]
    pub fn new() -> Self {
        Self {
            secrets: Vec::new(),
            working_directory: None,
        }
    }

    /// Append every supplied secret to the policy. Empty secrets are
    /// silently dropped — an empty `str::replace` needle would match
    /// between every byte and corrupt the output.
    ///
    /// Order does not matter: [`redact_pure`] iterates secrets *longest
    /// first* internally so a longer secret is rewritten before any of its
    /// substrings have a chance to mask it.
    #[must_use]
    pub fn with_secrets(mut self, secrets: &[&str]) -> Self {
        self.secrets
            .extend(secrets.iter().filter(|s| !s.is_empty()).map(|s| (*s).to_string()));
        self
    }

    /// Set the working directory used to classify detected paths. The
    /// caller is responsible for supplying an absolute path here; relative
    /// values are accepted but only match paths whose textual form happens
    /// to begin with the same prefix.
    #[must_use]
    pub fn with_working_directory(mut self, wd: impl Into<PathBuf>) -> Self {
        self.working_directory = Some(wd.into());
        self
    }
}

/// Rewrite `text` according to `policy`.
///
/// See the module-level documentation for the algorithm, the purity
/// contract, and the path-detection rule.
#[must_use]
pub fn redact_pure(text: &str, policy: &RedactionPolicy) -> String {
    // Pass 1: secret substitution. Process longest secrets first so an
    // outer secret that contains a shorter secret is fully redacted before
    // the shorter pattern has a chance to mask part of it.
    let pass1 = if policy.secrets.is_empty() {
        text.to_string()
    } else {
        let mut sorted: Vec<&str> = policy.secrets.iter().map(String::as_str).collect();
        // `sort_by_key` with `Reverse(len)` puts the longest needle first.
        // The relative ordering between equal-length secrets is
        // deterministic because `sort_by_key` is stable.
        sorted.sort_by_key(|s| std::cmp::Reverse(s.len()));
        let mut out = text.to_string();
        for needle in sorted {
            if !needle.is_empty() && out.contains(needle) {
                out = out.replace(needle, REDACTED);
            }
        }
        out
    };

    // Pass 2: path classification.
    classify_paths(&pass1, policy.working_directory.as_deref())
}

const REDACTED: &str = "<redacted>";
const EXTERNAL: &str = "<external>";

/// Walk `text` once and rewrite every detected absolute path. Bytes
/// between detected paths are copied verbatim.
fn classify_paths(text: &str, working_directory: Option<&std::path::Path>) -> String {
    let bytes = text.as_bytes();
    // Best-case capacity guess: identical to input. `<external>` shrinks
    // most paths, so the buffer rarely grows.
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut last_copy = 0usize;

    while cursor < bytes.len() {
        let at_boundary = cursor == 0 || !is_path_content_byte(bytes[cursor - 1]);
        if at_boundary {
            if let Some(end) = match_absolute_path_run(bytes, cursor) {
                // Flush the verbatim region before the detected path.
                out.push_str(&text[last_copy..cursor]);

                // Slicing on `cursor..end` is safe: every byte in that
                // range is by construction an ASCII path-content byte
                // (alphanumeric, `\`, `/`, `.`, `_`, `-`, `~`, `:`), so
                // the slice never bisects a multi-byte UTF-8 codepoint.
                let detected = &text[cursor..end];
                let replacement = classify_detected_path(detected, working_directory);
                out.push_str(&replacement);

                cursor = end;
                last_copy = end;
                continue;
            }
        }

        // Advance one UTF-8 codepoint at a time so we never bisect a
        // multi-byte sequence. Path-content bytes are pure ASCII, so this
        // always advances at least one byte.
        let lead = bytes[cursor];
        let step = utf8_codepoint_len(lead);
        cursor = (cursor + step).min(bytes.len());
    }

    // Flush the trailing verbatim region.
    out.push_str(&text[last_copy..]);
    out
}

/// Try to match an absolute-path run starting at byte offset `i`. Returns
/// the end offset (exclusive) of the matched path, or `None` if the bytes
/// at `i` do not begin a recognised absolute path.
fn match_absolute_path_run(bytes: &[u8], i: usize) -> Option<usize> {
    // Windows drive-letter pattern: `[A-Za-z]:[\\/]<one or more path bytes>`.
    if i + 3 <= bytes.len()
        && bytes[i].is_ascii_alphabetic()
        && bytes[i + 1] == b':'
        && (bytes[i + 2] == b'\\' || bytes[i + 2] == b'/')
    {
        let mut j = i + 3;
        while j < bytes.len() && is_path_content_byte(bytes[j]) {
            j += 1;
        }
        return Some(j);
    }

    // Unix absolute paths anchored at one of the closed-set prefixes. The
    // prefixes are deliberately specific so we don't scoop up bare `/`
    // tokens that happen to introduce a comment or a regex literal.
    const PREFIXES: &[&[u8]] = &[
        b"/Users/",
        b"/home/",
        b"/root/",
        b"/private/",
        b"/tmp/",
        b"/var/",
    ];
    for prefix in PREFIXES {
        if bytes.len() - i >= prefix.len() && &bytes[i..i + prefix.len()] == *prefix {
            let mut j = i + prefix.len();
            while j < bytes.len() && is_path_content_byte(bytes[j]) {
                j += 1;
            }
            return Some(j);
        }
    }

    None
}

/// Decide what string to emit in place of a detected absolute path.
fn classify_detected_path(detected: &str, working_directory: Option<&std::path::Path>) -> String {
    if let Some(wd) = working_directory {
        let wd_string = wd.to_string_lossy();
        let normalized_wd = normalize_path_for_compare(wd_string.as_ref());
        let normalized_path = normalize_path_for_compare(detected);

        // Strip a single trailing slash from the working directory so the
        // prefix comparison below is unambiguous.
        let trimmed_wd = trim_trailing_slash(&normalized_wd);

        // Path lies *inside* the working directory.
        if let Some(rest) = strip_path_prefix(&normalized_path, trimmed_wd) {
            // The relative form must use forward slashes regardless of
            // host platform — that is the byte-determinism requirement
            // (Requirement 2.1) — and must not carry a leading separator.
            let rel = rest.trim_start_matches('/');
            return rel.to_string();
        }
    }
    EXTERNAL.to_string()
}

/// Normalize a path for textual comparison: convert `\` to `/`, and lower
/// the drive letter at the start (so `C:\foo` and `c:/foo` compare equal).
///
/// All transformations swap one ASCII byte for another ASCII byte, so the
/// result is still valid UTF-8.
fn normalize_path_for_compare(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let has_drive_letter =
        bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    for (i, &b) in bytes.iter().enumerate() {
        let mut nb = if b == b'\\' { b'/' } else { b };
        if i == 0 && has_drive_letter {
            nb = nb.to_ascii_lowercase();
        }
        out.push(nb);
    }
    // SAFETY: every replacement above maps an ASCII byte to another ASCII
    // byte, and non-ASCII bytes (UTF-8 continuation bytes inside multi-
    // byte codepoints) are passed through untouched.
    String::from_utf8(out).expect("ASCII-only byte swaps preserve UTF-8")
}

/// Drop a single trailing `/` from `s` (but never strip `s` down to empty).
fn trim_trailing_slash(s: &str) -> &str {
    if s.len() > 1 && s.ends_with('/') {
        &s[..s.len() - 1]
    } else {
        s
    }
}

/// Return the suffix of `path` after `prefix`, but only when `path` is
/// equal to `prefix` or extends `prefix` with a `/`-separated segment.
/// Returns `None` when `path` is unrelated, or when `prefix` is the
/// textual prefix of `path` but the next byte after `prefix` is not `/`
/// (e.g. `c:/proj` vs `c:/projector` — those are different directories).
fn strip_path_prefix<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    if !path.starts_with(prefix) {
        return None;
    }
    let rest = &path[prefix.len()..];
    if rest.is_empty() {
        return Some("");
    }
    if rest.starts_with('/') {
        return Some(rest);
    }
    None
}

/// `true` iff `b` is one of the bytes we treat as belonging to the
/// interior of a filesystem path — ASCII alphanumerics plus the small
/// punctuation set common to both Windows and Unix path syntax.
const fn is_path_content_byte(b: u8) -> bool {
    matches!(b,
        b'a'..=b'z'
        | b'A'..=b'Z'
        | b'0'..=b'9'
        | b'\\'
        | b'/'
        | b'.'
        | b'_'
        | b'-'
        | b'~'
    )
}

/// Length in bytes of the UTF-8 codepoint that starts with `lead`. ASCII
/// returns 1; multi-byte sequences return 2–4. Defensive fallback of 1
/// for malformed leading bytes — callers feed `&str` so that branch is
/// unreachable in practice but keeps the helper total.
const fn utf8_codepoint_len(lead: u8) -> usize {
    if lead < 0x80 {
        1
    } else if lead < 0xC0 {
        1
    } else if lead < 0xE0 {
        2
    } else if lead < 0xF0 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy_with_secrets(secrets: &[&str]) -> RedactionPolicy {
        RedactionPolicy::new().with_secrets(secrets)
    }

    // -- pass 1: secret substitution ---------------------------------------

    #[test]
    fn empty_policy_returns_input_verbatim() {
        let policy = RedactionPolicy::new();
        let input = "hello world\nline 2\n";
        assert_eq!(redact_pure(input, &policy), input);
    }

    #[test]
    fn single_secret_is_replaced_with_redacted_marker() {
        let policy = policy_with_secrets(&["ABC123"]);
        assert_eq!(
            redact_pure("api_key=ABC123", &policy),
            "api_key=<redacted>",
        );
    }

    #[test]
    fn multiple_non_overlapping_secrets_are_each_replaced() {
        let policy = policy_with_secrets(&["sk-aaa", "sk-bbb"]);
        let input = "first=sk-aaa,second=sk-bbb,third=sk-aaa";
        let expected = "first=<redacted>,second=<redacted>,third=<redacted>";
        assert_eq!(redact_pure(input, &policy), expected);
    }

    #[test]
    fn overlapping_secrets_are_redacted_longest_first() {
        // With secrets ["AB", "ABCD"] and input "xABCDy", processing the
        // shorter one first would yield "x<redacted>CDy", masking only the
        // first two bytes of the longer secret. Longest-first is the
        // contract, so the output must be the fully redacted form.
        let policy = policy_with_secrets(&["AB", "ABCD"]);
        assert_eq!(redact_pure("xABCDy", &policy), "x<redacted>y");
    }

    #[test]
    fn empty_string_secret_is_silently_dropped() {
        let policy = policy_with_secrets(&["", "sk-real"]);
        // No infinite loop, no inserted markers between every byte.
        let input = "value=sk-real,trail";
        assert_eq!(redact_pure(input, &policy), "value=<redacted>,trail");
    }

    #[test]
    fn only_empty_string_secret_is_no_op() {
        let policy = policy_with_secrets(&[""]);
        assert_eq!(redact_pure("anything goes", &policy), "anything goes");
    }

    #[test]
    fn multibyte_utf8_input_is_not_bisected_when_secret_is_ascii() {
        // The renderer must locate "ABC123" inside the multi-byte text
        // without slicing through a CJK codepoint.
        let policy = policy_with_secrets(&["ABC123"]);
        let input = "中文 ABC123 中文";
        let output = redact_pure(input, &policy);
        assert_eq!(output, "中文 <redacted> 中文");
        // Sanity: the multi-byte content was preserved byte-for-byte.
        assert!(output.contains("中文"));
    }

    // -- pass 2: path classification ---------------------------------------

    #[test]
    fn windows_path_outside_working_directory_becomes_external() {
        let policy =
            RedactionPolicy::new().with_working_directory(std::path::PathBuf::from(r"D:\proj"));
        let input = r"loaded C:\Users\alice\data.csv";
        let output = redact_pure(input, &policy);
        assert!(
            output.contains("<external>"),
            "expected <external> in output, got: {output}"
        );
        assert!(
            !output.contains(r"C:\Users\alice"),
            "raw absolute path leaked: {output}"
        );
    }

    #[test]
    fn unix_path_outside_working_directory_becomes_external() {
        let policy =
            RedactionPolicy::new().with_working_directory(std::path::PathBuf::from("/proj"));
        let input = "loaded /home/alice/data.csv";
        let output = redact_pure(input, &policy);
        assert_eq!(output, "loaded <external>");
    }

    #[test]
    fn unix_path_inside_working_directory_renders_relative_with_forward_slashes() {
        let policy = RedactionPolicy::new()
            .with_working_directory(std::path::PathBuf::from("/home/alice/proj"));
        let input = "loaded /home/alice/proj/inputs/data.csv";
        let output = redact_pure(input, &policy);
        assert_eq!(output, "loaded inputs/data.csv");
    }

    #[test]
    fn windows_path_inside_working_directory_renders_relative_with_forward_slashes() {
        let policy =
            RedactionPolicy::new().with_working_directory(std::path::PathBuf::from(r"C:\proj"));
        let input = r"loaded C:\proj\subdir\data.csv";
        let output = redact_pure(input, &policy);
        assert_eq!(output, "loaded subdir/data.csv");
        assert!(!output.contains('\\'), "relative form must use `/` only");
    }

    #[test]
    fn windows_drive_letter_case_difference_still_matches_working_directory() {
        // Working directory uses uppercase drive letter; detected path
        // happens to use lowercase. The comparison must normalize the
        // drive-letter case before deciding inside vs. outside.
        let policy =
            RedactionPolicy::new().with_working_directory(std::path::PathBuf::from(r"C:\proj"));
        let input = r"opened c:/proj/data.csv";
        let output = redact_pure(input, &policy);
        assert_eq!(output, "opened data.csv");
    }

    #[test]
    fn no_working_directory_set_marks_every_detected_path_external() {
        let policy = RedactionPolicy::new();
        let input = "a=/home/alice/x.csv b=C:\\Users\\bob\\y.csv";
        let output = redact_pure(input, &policy);
        assert_eq!(output, "a=<external> b=<external>");
    }

    #[test]
    fn url_with_home_substring_is_not_misclassified_as_path() {
        // The bytes between `://` and `/home/` are all path-content bytes,
        // so the inner `/home/` is not at a boundary and the scanner skips
        // it.
        let policy =
            RedactionPolicy::new().with_working_directory(std::path::PathBuf::from("/anywhere"));
        let input = "see https://example.com/home/alice/data";
        let output = redact_pure(input, &policy);
        assert_eq!(output, input);
    }

    #[test]
    fn relative_path_in_input_is_left_alone() {
        let policy =
            RedactionPolicy::new().with_working_directory(std::path::PathBuf::from("/proj"));
        let input = "see ./data.csv and ../parent/x.txt";
        let output = redact_pure(input, &policy);
        assert_eq!(output, input);
    }

    // -- determinism / structural properties --------------------------------

    #[test]
    fn applying_redact_pure_twice_is_a_no_op() {
        let policy = RedactionPolicy::new()
            .with_secrets(&["sk-XYZ"])
            .with_working_directory(std::path::PathBuf::from("/home/alice/proj"));
        let input =
            "key=sk-XYZ outside=/Users/eve/leak.csv inside=/home/alice/proj/data.csv";
        let once = redact_pure(input, &policy);
        let twice = redact_pure(&once, &policy);
        assert_eq!(once, twice, "redact_pure must be idempotent");
    }

    #[test]
    fn lf_line_endings_are_preserved_and_no_cr_is_introduced() {
        let policy = policy_with_secrets(&["sk-aaa"]);
        let input = "line1\nkey=sk-aaa\nline3\n";
        let output = redact_pure(input, &policy);
        assert!(!output.contains('\r'), "no CR may be introduced");
        assert_eq!(
            output.matches('\n').count(),
            input.matches('\n').count(),
            "LF count must be preserved"
        );
        assert_eq!(output, "line1\nkey=<redacted>\nline3\n");
    }

    #[test]
    fn detection_at_start_of_string_works() {
        let policy = RedactionPolicy::new();
        let input = "/home/alice/data.csv";
        let output = redact_pure(input, &policy);
        assert_eq!(output, "<external>");
    }

    #[test]
    fn detection_at_end_of_string_works() {
        let policy = RedactionPolicy::new();
        let input = "tail /var/log/x.log";
        let output = redact_pure(input, &policy);
        assert_eq!(output, "tail <external>");
    }

    #[test]
    fn multiple_paths_in_one_input_are_each_classified() {
        let policy = RedactionPolicy::new()
            .with_working_directory(std::path::PathBuf::from("/home/alice/proj"));
        let input =
            "in=/home/alice/proj/a.csv ext=/Users/bob/b.csv also=/home/alice/proj/sub/c.txt";
        let output = redact_pure(input, &policy);
        assert_eq!(output, "in=a.csv ext=<external> also=sub/c.txt");
    }

    #[test]
    fn secret_substitution_runs_before_path_classification() {
        // If a secret happened to encode an absolute path, pass 1 would
        // remove it before pass 2 even saw it.
        let policy = policy_with_secrets(&["/home/alice/proj/secret.txt"]);
        let input = "leaked=/home/alice/proj/secret.txt rest";
        let output = redact_pure(input, &policy);
        assert_eq!(output, "leaked=<redacted> rest");
    }

    #[test]
    fn policy_builder_chain_accumulates_secrets() {
        let policy = RedactionPolicy::new()
            .with_secrets(&["alpha"])
            .with_secrets(&["beta"]);
        let input = "alpha and beta";
        let output = redact_pure(input, &policy);
        assert_eq!(output, "<redacted> and <redacted>");
    }

    // -- helper coverage ----------------------------------------------------

    #[test]
    fn normalize_path_lowercases_drive_letter_and_swaps_separators() {
        assert_eq!(normalize_path_for_compare(r"C:\proj\sub"), "c:/proj/sub");
        assert_eq!(normalize_path_for_compare("c:/proj/sub"), "c:/proj/sub");
        assert_eq!(normalize_path_for_compare("/home/alice"), "/home/alice");
    }

    #[test]
    fn strip_path_prefix_requires_segment_boundary() {
        // `/home/alice/projector/x` must NOT be classified as inside
        // `/home/alice/proj` — that would be a substring match, not a
        // segment match.
        assert_eq!(
            strip_path_prefix("/home/alice/projector/x", "/home/alice/proj"),
            None,
        );
        assert_eq!(
            strip_path_prefix("/home/alice/proj/x", "/home/alice/proj"),
            Some("/x"),
        );
        assert_eq!(
            strip_path_prefix("/home/alice/proj", "/home/alice/proj"),
            Some(""),
        );
    }

    #[test]
    fn projector_directory_is_not_treated_as_inside_proj() {
        // End-to-end version of the segment-boundary check above.
        let policy =
            RedactionPolicy::new().with_working_directory(std::path::PathBuf::from("/home/alice/proj"));
        let input = "loaded /home/alice/projector/x.csv";
        let output = redact_pure(input, &policy);
        assert_eq!(output, "loaded <external>");
    }
}
