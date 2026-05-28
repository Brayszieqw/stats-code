//! Pure template-engine renderer for sidecar snippets.
//!
//! Feature: parity-and-multilang-sidecar — task 2.3.
//!
//! `render_pure` is the deterministic, side-effect-free string renderer that
//! powers every sidecar snippet body. Together with `header::format_header`
//! it is the only code path between embedded `*.tmpl.txt` templates and the
//! bytes returned by `generate_snippet` (task 2.5), so its purity contract
//! is what ultimately lets Requirement 2.1 (byte-identical output across
//! hosts and clocks) hold.
//!
//! # Placeholder grammar (closed set)
//!
//! ```text
//! {{dataset.sha256}}        → the supplied 64-hex lowercase SHA256
//! {{release.version}}       → the supplied Stats Code release version
//! {{column.<i>.name}}       → columns[<i>].name  (caller-supplied order)
//! {{column.<i>.dtype}}      → columns[<i>].dtype.as_token()
//!                             (one of `numeric | categorical | date | string`)
//! {{params.<key>}}          → params[<key>]
//! ```
//!
//! Any other placeholder shape (`{{nonsense}}`, `{{column.0.unknown}}`,
//! `{{params.}}`, …) returns a [`RenderError`] — the renderer never
//! silently swallows an unknown directive.
//!
//! # Whitespace policy: strict
//!
//! Inner whitespace is **not** trimmed. `{{ dataset.sha256 }}` (note the
//! spaces) is rejected as [`RenderError::UnknownPlaceholder`]. Template
//! authors control the exact spelling, so a stray space is a bug worth
//! surfacing rather than masking. See the
//! `whitespace_inside_braces_is_rejected_strictly` test for the canonical
//! example.
//!
//! # Tokenizer disambiguation
//!
//! The scanner uses the simplest possible rule: the first `{{` opens, the
//! next `}}` closes. Triple-brace patterns like `{{{x}}}` are therefore
//! parsed as `{{` (open) → inner `{x` → `}}` (close), which is rejected as
//! [`RenderError::UnknownPlaceholder`]. Wave-1 sidecar templates never need
//! literal `{{` in the rendered output, and the strict tokenizer keeps the
//! determinism story local.
//!
//! # Purity contract
//!
//! `render_pure` reads no clock, no environment variable, no random
//! source, opens no file, and acquires no lock. Every byte of the output
//! is derived from the four explicit inputs (template, params, columns,
//! `dataset_sha256`, `release_version`). Two invocations on different hosts at
//! different times with structurally identical inputs return byte-identical
//! `String`s — the upstream `generate_snippet` orchestration relies on this
//! to satisfy Requirement 2.1.
//!
//! # Line endings
//!
//! The renderer copies bytes verbatim outside of `{{ … }}` placeholders.
//! It does **not** normalize line endings: a template carrying `\r` would
//! emit `\r`. Wave-1 templates are LF-only on disk by convention, so the
//! rendered output is LF-only by transitivity. Silently rewriting line
//! endings here would hide drift between the embedded template files and
//! the LF guarantee of Requirement 2.1, so we deliberately do not.

use std::collections::BTreeMap;

use thiserror::Error;

use super::header::Column;

/// Mapping from `{{params.<key>}}` keys to their substituted string values.
///
/// `BTreeMap` is chosen for two reasons:
///
/// 1. Iteration order is deterministic, which matters whenever the caller
///    serializes the map into another deterministic artifact (the renderer
///    itself never iterates the map — it only does `get(<key>)` lookups
///    during placeholder substitution, but using a sorted map keeps the
///    determinism story uniform across the sidecar pipeline).
/// 2. It avoids pulling in a hashing crate solely for parameter lookup,
///    keeping the dependency graph minimal.
pub type RenderParams = BTreeMap<String, String>;

/// Errors returned by [`render_pure`] when the template uses a placeholder
/// shape that falls outside the closed grammar documented at the module
/// level.
///
/// All variants carry enough context to point a template author at the
/// exact offending fragment without leaking host state — error messages
/// are pure functions of the offending input.
#[derive(Debug, Error, Eq, PartialEq, Clone)]
pub enum RenderError {
    /// A `{{ … }}` block whose inner directive does not match any of the
    /// five supported placeholder shapes.
    #[error("unknown placeholder: {{{{{placeholder}}}}}")]
    UnknownPlaceholder {
        /// The verbatim inner text (between the opening `{{` and closing
        /// `}}`), with no whitespace trimming applied.
        placeholder: String,
    },

    /// `{{` opened without a matching `}}` close, or the inner directive
    /// was empty (`{{}}`).
    #[error("malformed placeholder fragment: {fragment:?}")]
    MalformedPlaceholder {
        /// The offending substring, starting at the unterminated `{{` (or
        /// the empty `{{}}` pair) and running to the end of the template
        /// or to the empty close — short enough to print, never the whole
        /// template.
        fragment: String,
    },

    /// `{{column.<i>.…}}` referenced an index past the end of the
    /// caller-supplied column slice.
    #[error("column index {index} is out of range (have {len} columns)")]
    ColumnIndexOutOfRange {
        /// The numeric index parsed from the placeholder.
        index: usize,
        /// The length of the column slice supplied to [`render_pure`].
        len: usize,
    },

    /// `{{column.<i>.…}}` carried a non-integer index.
    #[error("column index is not a non-negative integer: {token:?}")]
    ColumnIndexNotInteger {
        /// The verbatim slot text that failed `usize::from_str`.
        token: String,
    },

    /// `{{params.<key>}}` referenced a key that does not exist in the
    /// supplied [`RenderParams`].
    #[error("missing param: {key:?}")]
    MissingParam {
        /// The verbatim parameter key looked up against [`RenderParams`].
        key: String,
    },
}

/// Render a sidecar template into its final UTF-8 string form.
///
/// See the module-level documentation for the placeholder grammar, the
/// strict whitespace policy, the tokenizer disambiguation rule, and the
/// purity contract.
///
/// # Errors
///
/// Returns a [`RenderError`] when the template uses a placeholder shape
/// that falls outside the closed grammar, references an out-of-range
/// column, names a missing parameter, or contains a malformed `{{ … }}`
/// fragment.
pub fn render_pure(
    template: &str,
    params: &RenderParams,
    columns: &[Column],
    dataset_sha256: &str,
    release_version: &str,
) -> Result<String, RenderError> {
    // Reserve a generous buffer to keep the typical render allocation-free
    // past the first reserve. 64 extra bytes covers most short SHA256 +
    // version substitutions; longer expansions reallocate in amortized
    // O(1).
    let mut out = String::with_capacity(template.len() + 64);

    let bytes = template.as_bytes();
    let mut cursor = 0usize;

    while cursor < bytes.len() {
        // Look for the next `{{` opener. UTF-8 safety: `{` is ASCII (0x7B)
        // and never appears as a continuation byte, so byte-level search
        // and slicing on `{{` / `}}` boundaries cannot bisect a multi-byte
        // codepoint.
        if cursor + 1 < bytes.len() && bytes[cursor] == b'{' && bytes[cursor + 1] == b'{' {
            // Find the closing `}}` starting after the opener.
            let inner_start = cursor + 2;
            let close = find_double_brace_close(bytes, inner_start);

            match close {
                None => {
                    // Unterminated `{{` — copy at most a short tail into
                    // the error message rather than the whole template.
                    let tail_end = (cursor + 32).min(bytes.len());
                    return Err(RenderError::MalformedPlaceholder {
                        fragment: template[cursor..tail_end].to_string(),
                    });
                }
                Some(close_idx) => {
                    let inner = &template[inner_start..close_idx];
                    if inner.is_empty() {
                        return Err(RenderError::MalformedPlaceholder {
                            fragment: "{{}}".to_string(),
                        });
                    }
                    substitute_placeholder(
                        inner,
                        params,
                        columns,
                        dataset_sha256,
                        release_version,
                        &mut out,
                    )?;
                    // Skip past the closing `}}`.
                    cursor = close_idx + 2;
                }
            }
        } else {
            // Copy one UTF-8 codepoint verbatim. We advance by the byte
            // length of the leading codepoint so we never bisect a
            // multi-byte sequence; finding the codepoint length from a
            // leading byte is cheap and avoids allocating a `chars`
            // iterator.
            let lead = bytes[cursor];
            let codepoint_len = utf8_codepoint_len(lead);
            let end = (cursor + codepoint_len).min(bytes.len());
            out.push_str(&template[cursor..end]);
            cursor = end;
        }
    }

    Ok(out)
}

/// Locate the next `}}` occurrence at or after `from`. Returns the byte
/// offset of the first `}` of the pair, or `None` if no `}}` exists.
fn find_double_brace_close(bytes: &[u8], from: usize) -> Option<usize> {
    if from >= bytes.len() {
        return None;
    }
    let mut i = from;
    while i + 1 < bytes.len() {
        if bytes[i] == b'}' && bytes[i + 1] == b'}' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Length in bytes of the UTF-8 codepoint that starts with `lead`. Returns
/// 1 for ASCII (the common case) and 1–4 for multi-byte codepoints. Falls
/// back to 1 for malformed leading bytes — `template: &str` is guaranteed
/// to be valid UTF-8 by the borrow checker, so the fallback is unreachable
/// in practice but keeps the function total.
const fn utf8_codepoint_len(lead: u8) -> usize {
    if lead < 0x80 {
        1
    } else if lead < 0xC0 {
        1 // continuation byte; defensive — `&str` invariants forbid this
    } else if lead < 0xE0 {
        2
    } else if lead < 0xF0 {
        3
    } else {
        4
    }
}

/// Resolve a single `{{ … }}` directive (whose verbatim inner text is
/// `inner`) and append the substituted value to `out`.
fn substitute_placeholder(
    inner: &str,
    params: &RenderParams,
    columns: &[Column],
    dataset_sha256: &str,
    release_version: &str,
    out: &mut String,
) -> Result<(), RenderError> {
    // Exact-match dispatch first. Strict — no whitespace trimming.
    match inner {
        "dataset.sha256" => {
            out.push_str(dataset_sha256);
            return Ok(());
        }
        "release.version" => {
            out.push_str(release_version);
            return Ok(());
        }
        _ => {}
    }

    if let Some(rest) = inner.strip_prefix("column.") {
        return substitute_column(rest, columns, out);
    }

    if let Some(key) = inner.strip_prefix("params.") {
        return substitute_param(key, params, out);
    }

    Err(RenderError::UnknownPlaceholder {
        placeholder: inner.to_string(),
    })
}

/// Resolve a `{{column.<i>.<field>}}` directive whose `<i>.<field>`
/// suffix is `rest`.
fn substitute_column(
    rest: &str,
    columns: &[Column],
    out: &mut String,
) -> Result<(), RenderError> {
    // Split on the first `.` to separate the index from the field name.
    let (idx_str, field) = rest.split_once('.').ok_or_else(|| {
        // No second `.` at all — the directive does not match any column
        // shape. Surfaces as `UnknownPlaceholder` because we cannot tell
        // the user whether they meant `name` or `dtype`.
        RenderError::UnknownPlaceholder {
            placeholder: format!("column.{rest}"),
        }
    })?;

    if idx_str.is_empty() {
        return Err(RenderError::ColumnIndexNotInteger {
            token: idx_str.to_string(),
        });
    }

    let index: usize = idx_str
        .parse()
        .map_err(|_| RenderError::ColumnIndexNotInteger {
            token: idx_str.to_string(),
        })?;

    let column = columns
        .get(index)
        .ok_or(RenderError::ColumnIndexOutOfRange {
            index,
            len: columns.len(),
        })?;

    match field {
        "name" => {
            out.push_str(&column.name);
            Ok(())
        }
        "dtype" => {
            out.push_str(column.dtype.as_token());
            Ok(())
        }
        _ => Err(RenderError::UnknownPlaceholder {
            placeholder: format!("column.{idx_str}.{field}"),
        }),
    }
}

/// Resolve a `{{params.<key>}}` directive whose `<key>` is `key`.
fn substitute_param(
    key: &str,
    params: &RenderParams,
    out: &mut String,
) -> Result<(), RenderError> {
    if key.is_empty() || !is_valid_param_key(key) {
        return Err(RenderError::UnknownPlaceholder {
            placeholder: format!("params.{key}"),
        });
    }

    let value = params.get(key).ok_or_else(|| RenderError::MissingParam {
        key: key.to_string(),
    })?;
    out.push_str(value);
    Ok(())
}

/// Allowed parameter key alphabet: ASCII alphanumerics + `_` + `.`.
///
/// Permitting `.` lets future templates address nested keys via the
/// literal `{{params.foo.bar}}` form; the renderer simply looks up the
/// flat string `"foo.bar"` in [`RenderParams`].
const fn is_valid_param_key_byte(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'.')
}

fn is_valid_param_key(key: &str) -> bool {
    key.bytes().all(is_valid_param_key_byte)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::header::ColumnDtype;

    /// Canonical 64-hex SHA256 fixture used across every render test that
    /// does not specifically exercise SHA256 invariants.
    const SHA256: &str =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn cols() -> Vec<Column> {
        vec![
            Column {
                name: "age".into(),
                dtype: ColumnDtype::Numeric,
            },
            Column {
                name: "sex".into(),
                dtype: ColumnDtype::Categorical,
            },
        ]
    }

    fn empty_params() -> RenderParams {
        RenderParams::new()
    }

    // -- happy paths --------------------------------------------------------

    #[test]
    fn empty_template_yields_empty_output() {
        let out = render_pure("", &empty_params(), &cols(), SHA256, "0.5.0").unwrap();
        assert_eq!(out, "");
    }

    #[test]
    fn template_without_placeholders_is_byte_identical() {
        let template = "library(survival)\nfit <- coxph(...)\n# trailing";
        let out = render_pure(template, &empty_params(), &cols(), SHA256, "0.5.0").unwrap();
        assert_eq!(out, template);
    }

    #[test]
    fn dataset_sha256_placeholder_substitutes_supplied_hex() {
        let out = render_pure(
            "sha={{dataset.sha256}}",
            &empty_params(),
            &cols(),
            SHA256,
            "0.5.0",
        )
        .unwrap();
        assert_eq!(out, format!("sha={SHA256}"));
    }

    #[test]
    fn release_version_placeholder_substitutes_supplied_version() {
        let out = render_pure(
            "v={{release.version}}",
            &empty_params(),
            &cols(),
            SHA256,
            "1.2.3-rc.4",
        )
        .unwrap();
        assert_eq!(out, "v=1.2.3-rc.4");
    }

    #[test]
    fn column_name_and_dtype_resolve_in_caller_supplied_order() {
        let out = render_pure(
            "first={{column.0.name}} second_dtype={{column.1.dtype}}",
            &empty_params(),
            &cols(),
            SHA256,
            "0.5.0",
        )
        .unwrap();
        assert_eq!(out, "first=age second_dtype=categorical");
    }

    #[test]
    fn params_placeholder_resolves_to_looked_up_value() {
        let mut p = RenderParams::new();
        p.insert("alpha".to_string(), "0.05".to_string());
        let out = render_pure("a={{params.alpha}}", &p, &cols(), SHA256, "0.5.0").unwrap();
        assert_eq!(out, "a=0.05");
    }

    #[test]
    fn dotted_param_key_is_looked_up_verbatim() {
        let mut p = RenderParams::new();
        p.insert("nested.key".to_string(), "value".to_string());
        let out = render_pure("x={{params.nested.key}}", &p, &cols(), SHA256, "0.5.0").unwrap();
        assert_eq!(out, "x=value");
    }

    #[test]
    fn multiple_placeholders_inline_all_substitute() {
        let mut p = RenderParams::new();
        p.insert("pkg".to_string(), "tableone".to_string());
        let template =
            r#"library({{params.pkg}}); read_csv("data.csv") # {{dataset.sha256}}"#;
        let out = render_pure(template, &p, &cols(), SHA256, "0.5.0").unwrap();
        assert_eq!(
            out,
            format!(r#"library(tableone); read_csv("data.csv") # {SHA256}"#),
        );
    }

    #[test]
    fn render_is_idempotent_byte_for_byte() {
        let mut p = RenderParams::new();
        p.insert("alpha".to_string(), "0.05".to_string());
        let template =
            "v={{release.version}}\nsha={{dataset.sha256}}\nfirst={{column.0.name}}\na={{params.alpha}}\n";
        let a = render_pure(template, &p, &cols(), SHA256, "0.5.0").unwrap();
        let b = render_pure(template, &p, &cols(), SHA256, "0.5.0").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn lf_line_endings_preserved_and_no_cr_introduced() {
        let template = "line1\nline2\n{{column.0.name}}\nline4\n";
        let out = render_pure(template, &empty_params(), &cols(), SHA256, "0.5.0").unwrap();
        assert!(!out.contains('\r'), "render must not introduce CR bytes");
        assert_eq!(out.matches('\n').count(), template.matches('\n').count());
        assert_eq!(out, "line1\nline2\nage\nline4\n");
    }

    // -- error paths --------------------------------------------------------

    #[test]
    fn unknown_placeholder_error_for_unrecognized_directive() {
        let err = render_pure("{{nonsense}}", &empty_params(), &cols(), SHA256, "0.5.0")
            .unwrap_err();
        assert_eq!(
            err,
            RenderError::UnknownPlaceholder {
                placeholder: "nonsense".to_string(),
            },
        );
    }

    #[test]
    fn malformed_placeholder_error_when_close_is_missing() {
        let err = render_pure("prefix {{open", &empty_params(), &cols(), SHA256, "0.5.0")
            .unwrap_err();
        match err {
            RenderError::MalformedPlaceholder { fragment } => {
                assert!(
                    fragment.starts_with("{{"),
                    "fragment should begin at the offending opener: {fragment:?}"
                );
            }
            other => panic!("expected MalformedPlaceholder, got {other:?}"),
        }
    }

    #[test]
    fn malformed_placeholder_error_when_inner_is_empty() {
        let err = render_pure("{{}}", &empty_params(), &cols(), SHA256, "0.5.0").unwrap_err();
        assert_eq!(
            err,
            RenderError::MalformedPlaceholder {
                fragment: "{{}}".to_string(),
            },
        );
    }

    #[test]
    fn column_index_out_of_range_when_index_exceeds_length() {
        let err = render_pure(
            "{{column.5.name}}",
            &empty_params(),
            &cols(),
            SHA256,
            "0.5.0",
        )
        .unwrap_err();
        assert_eq!(
            err,
            RenderError::ColumnIndexOutOfRange {
                index: 5,
                len: 2,
            },
        );
    }

    #[test]
    fn column_index_not_integer_when_index_is_alphabetic() {
        let err = render_pure(
            "{{column.abc.name}}",
            &empty_params(),
            &cols(),
            SHA256,
            "0.5.0",
        )
        .unwrap_err();
        assert_eq!(
            err,
            RenderError::ColumnIndexNotInteger {
                token: "abc".to_string(),
            },
        );
    }

    #[test]
    fn column_index_not_integer_when_index_is_negative() {
        let err = render_pure(
            "{{column.-1.name}}",
            &empty_params(),
            &cols(),
            SHA256,
            "0.5.0",
        )
        .unwrap_err();
        // `-1` doesn't parse as `usize`, so it's surfaced as a non-integer
        // index rather than out-of-range.
        assert_eq!(
            err,
            RenderError::ColumnIndexNotInteger {
                token: "-1".to_string(),
            },
        );
    }

    #[test]
    fn column_unknown_field_is_unknown_placeholder() {
        let err = render_pure(
            "{{column.0.unknown}}",
            &empty_params(),
            &cols(),
            SHA256,
            "0.5.0",
        )
        .unwrap_err();
        assert_eq!(
            err,
            RenderError::UnknownPlaceholder {
                placeholder: "column.0.unknown".to_string(),
            },
        );
    }

    #[test]
    fn missing_param_error_when_key_not_in_params() {
        let err = render_pure(
            "{{params.missing}}",
            &empty_params(),
            &cols(),
            SHA256,
            "0.5.0",
        )
        .unwrap_err();
        assert_eq!(
            err,
            RenderError::MissingParam {
                key: "missing".to_string(),
            },
        );
    }

    #[test]
    fn empty_param_key_is_unknown_placeholder() {
        let err = render_pure("{{params.}}", &empty_params(), &cols(), SHA256, "0.5.0")
            .unwrap_err();
        assert_eq!(
            err,
            RenderError::UnknownPlaceholder {
                placeholder: "params.".to_string(),
            },
        );
    }

    #[test]
    fn whitespace_inside_braces_is_rejected_strictly() {
        // Strict policy: no trimming. `{{ params.x }}` is unknown because
        // its literal inner text is " params.x " (with surrounding
        // spaces), which doesn't match any closed-grammar shape.
        let mut p = RenderParams::new();
        p.insert("x".to_string(), "1".to_string());
        let err = render_pure("{{ params.x }}", &p, &cols(), SHA256, "0.5.0").unwrap_err();
        assert_eq!(
            err,
            RenderError::UnknownPlaceholder {
                placeholder: " params.x ".to_string(),
            },
        );
    }

    // -- caller-supplied iteration order is preserved -----------------------

    #[test]
    fn column_iteration_does_not_resort_input() {
        // Reverse-order columns: index 0 must still be the first slice
        // element, regardless of any alphabetical sorting that another
        // engine might apply.
        let cols = vec![
            Column {
                name: "zeta".into(),
                dtype: ColumnDtype::String,
            },
            Column {
                name: "alpha".into(),
                dtype: ColumnDtype::Numeric,
            },
        ];
        let out = render_pure(
            "{{column.0.name}}|{{column.1.name}}",
            &empty_params(),
            &cols,
            SHA256,
            "0.5.0",
        )
        .unwrap();
        assert_eq!(out, "zeta|alpha");
    }

    // -- determinism with non-empty params ---------------------------------

    #[test]
    fn two_param_lookups_share_one_value() {
        let mut p = RenderParams::new();
        p.insert("alpha".to_string(), "0.05".to_string());
        let out = render_pure(
            "{{params.alpha}}{{params.alpha}}",
            &p,
            &cols(),
            SHA256,
            "0.5.0",
        )
        .unwrap();
        assert_eq!(out, "0.050.05");
    }
}
