// ---------------------------------------------------------------------------
// Shared helper functions used across multiple handler modules.
// ---------------------------------------------------------------------------

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use calamine::{open_workbook_auto, Data, Reader};
use serde_json::Value;

use crate::schema::is_missing_value;

// ---------------------------------------------------------------------------
// Data access helpers
// ---------------------------------------------------------------------------

/// Look up a column index by name, or return a descriptive error.
pub(crate) fn require_column(index: &BTreeMap<String, usize>, name: &str) -> Result<usize, String> {
    index
        .get(name)
        .copied()
        .ok_or_else(|| format!("Column `{name}` was not found in the dataset header."))
}

/// Normalize a group value, mapping missing sentinels to `<missing>`.
pub(crate) fn normalize_group_value(value: &str) -> String {
    let trimmed = value.trim();
    if is_missing_value(trimmed) {
        "<missing>".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Parse an event indicator that may be 0/1, true/false, yes/no.
pub(crate) fn parse_event_value(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if is_missing_value(trimmed) {
        return None;
    }
    match trimmed.to_ascii_lowercase().as_str() {
        "true" | "yes" | "y" => Some(1.0),
        "false" | "no" | "n" => Some(0.0),
        _ => trimmed.parse::<f64>().ok(),
    }
}

// ---------------------------------------------------------------------------
// String / collection helpers
// ---------------------------------------------------------------------------

/// Merge two slices of strings, deduplicating while preserving order.
pub(crate) fn merge_unique_strings(
    primary: &[String],
    secondary: &[String],
    exclude: &[String],
) -> Vec<String> {
    let excluded = exclude
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut seen = std::collections::BTreeSet::new();
    primary
        .iter()
        .chain(secondary.iter())
        .filter(|value| !excluded.contains(*value))
        .filter(|value| seen.insert((*value).clone()))
        .cloned()
        .collect()
}

/// Join a list of strings with ` + `, or return a placeholder if empty.
pub(crate) fn join_or_placeholder(values: &[String], placeholder: &str) -> String {
    if values.is_empty() {
        placeholder.to_string()
    } else {
        values.join(" + ")
    }
}

/// Convert any `Display` error into a `String`.
pub(crate) fn stringify_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

// ---------------------------------------------------------------------------
// Time helpers
// ---------------------------------------------------------------------------

/// Current unix timestamp in nanoseconds.
pub(crate) fn unix_timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

// ---------------------------------------------------------------------------
// File / path helpers
// ---------------------------------------------------------------------------

/// FNV-1a 64-bit hash of a file's contents, returned as a hex string.
pub(crate) fn fingerprint_file(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    Some(fnv1a64_hex(&bytes))
}

/// FNV-1a 64-bit hash of a byte slice.
pub(crate) fn fnv1a64_hex(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// Extract the first matching string field from a JSON value.
pub(crate) fn extract_string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::to_string)
}

/// Resolve a path to an absolute, canonicalized string for matching.
pub(crate) fn resolve_path_for_match(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_or_else(|_| path.to_path_buf(), |cwd| cwd.join(path))
    };
    absolute
        .canonicalize()
        .unwrap_or(absolute)
        .display()
        .to_string()
}

/// Resolve a raw path string using an optional base directory.
pub(crate) fn resolve_path_str_for_match(raw: &str, base_dir: Option<&Path>) -> String {
    let path = PathBuf::from(raw);
    let candidate = if path.is_absolute() {
        path
    } else if let Some(base_dir) = base_dir {
        base_dir.join(path)
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path.clone()))
            .unwrap_or(path)
    };
    candidate
        .canonicalize()
        .unwrap_or(candidate)
        .display()
        .to_string()
}

/// Normalize a path string for case-insensitive comparison on Windows.
pub(crate) fn path_match_key(path: &str) -> String {
    let trimmed = path
        .strip_prefix(r"\\?\")
        .or_else(|| path.strip_prefix(r"//?/"))
        .unwrap_or(path);
    if cfg!(windows) {
        trimmed.replace('/', "\\").to_lowercase()
    } else {
        trimmed.to_string()
    }
}

/// Check whether two paths refer to the same file.
pub(crate) fn path_matches(left: &str, right: &str) -> bool {
    path_match_key(left) == path_match_key(right)
}

// ---------------------------------------------------------------------------
// Excel conversion helpers
// ---------------------------------------------------------------------------

/// Convert an Excel file to a temporary CSV file.
pub(crate) fn excel_to_temp_csv(excel_path: &Path) -> Result<PathBuf, String> {
    let (headers, records) = read_excel_records(excel_path)?;
    let temp_dir = excel_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = excel_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("excel_data");
    let temp_path = temp_dir.join(format!(".{stem}_stats_code_tmp.csv"));
    let mut writer = csv::Writer::from_path(&temp_path).map_err(stringify_error)?;
    writer.write_record(&headers).map_err(stringify_error)?;
    for record in &records {
        writer.write_record(record).map_err(stringify_error)?;
    }
    writer.flush().map_err(stringify_error)?;
    Ok(temp_path)
}

/// Read all records from an Excel file (first sheet).
pub(crate) fn read_excel_records(excel_path: &Path) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let mut workbook = open_workbook_auto(excel_path)
        .map_err(|error| format!("Cannot open Excel file `{}`: {error}", excel_path.display()))?;
    let sheet_names = workbook.sheet_names().clone();
    let sheet_name = sheet_names
        .first()
        .ok_or_else(|| "Excel file contains no sheets.".to_string())?
        .clone();
    let range = workbook
        .worksheet_range(&sheet_name)
        .map_err(|error| format!("Cannot read sheet `{sheet_name}`: {error}"))?;
    let mut rows = range.rows();
    let header_row = rows
        .next()
        .ok_or_else(|| "Excel sheet has no header row.".to_string())?;
    let headers: Vec<String> = header_row.iter().map(excel_cell_to_string).collect();
    let records: Vec<Vec<String>> = rows
        .map(|row| row.iter().map(excel_cell_to_string).collect())
        .collect();
    Ok((headers, records))
}

/// Convert an Excel cell value to a string representation.
pub(crate) fn excel_cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => {
            // If the float is actually an integer, format without decimal
            if f.fract() == 0.0 && f.abs() < i64::MAX as f64 {
                format!("{}", *f as i64)
            } else {
                f.to_string()
            }
        }
        Data::Bool(b) => if *b { "1".to_string() } else { "0".to_string() },
        Data::Error(e) => format!("{e:?}"),
        Data::DateTime(f) => f.to_string(),
        Data::DateTimeIso(s) => s.clone(),
        Data::DurationIso(s) => s.clone(),
    }
}
