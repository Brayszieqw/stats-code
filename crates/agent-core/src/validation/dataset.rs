//! Dataset validation functions.
//!
//! Validates file extensions, size limits, non-empty constraints, and upload quotas.

use crate::models::{DatasetSummary, ErrorCode, ErrorPayload};

/// Returns `true` iff the file extension (case-insensitive) is in { csv, tsv, xlsx, xls }.
///
/// The extension is extracted from the last `.` in `name`.
#[must_use] 
pub fn is_supported_dataset_extension(name: &str) -> bool {
    let ext = match name.rsplit('.').next() {
        Some(e) if name.contains('.') => e,
        _ => return false,
    };
    matches!(ext.to_ascii_lowercase().as_str(), "csv" | "tsv" | "xlsx" | "xls")
}

/// Returns `Err(DatasetTooLarge)` iff `size_bytes > 50 MB` or `row_count > 1_000_000`.
pub fn validate_dataset_size(size_bytes: u64, row_count: u64) -> Result<(), ErrorPayload> {
    const MAX_SIZE: u64 = 50 * 1024 * 1024; // 50 MB
    const MAX_ROWS: u64 = 1_000_000;

    if size_bytes > MAX_SIZE {
        return Err(ErrorPayload {
            error_code: ErrorCode::DatasetTooLarge,
            message: format!("数据文件过大：文件大小 {size_bytes} 字节，超过上限 {MAX_SIZE} 字节"),
            details: None,
        });
    }
    if row_count > MAX_ROWS {
        return Err(ErrorPayload {
            error_code: ErrorCode::DatasetTooLarge,
            message: format!("数据文件过大：行数 {row_count}，超过上限 {MAX_ROWS} 行"),
            details: None,
        });
    }
    Ok(())
}

/// Returns `Err(DatasetEmpty)` iff `summary.columns.is_empty() || summary.row_count == 0`.
pub fn validate_dataset_non_empty(summary: &DatasetSummary) -> Result<(), ErrorPayload> {
    if summary.columns.is_empty() {
        return Err(ErrorPayload {
            error_code: ErrorCode::DatasetEmpty,
            message: "数据文件为空：列数为 0".to_string(),
            details: None,
        });
    }
    if summary.row_count == 0 {
        return Err(ErrorPayload {
            error_code: ErrorCode::DatasetEmpty,
            message: "数据文件为空：行数为 0".to_string(),
            details: None,
        });
    }
    Ok(())
}

/// Returns `Err(SessionQuotaExceeded)` iff `used + new_size > quota` (saturating addition).
pub fn check_upload_quota(used: u64, new_size: u64, quota: u64) -> Result<(), ErrorPayload> {
    if used.saturating_add(new_size) > quota {
        let quota_mb = quota / (1024 * 1024);
        return Err(ErrorPayload {
            error_code: ErrorCode::SessionQuotaExceeded,
            message: format!("本会话上传容量已满（{quota_mb} MB）"),
            details: None,
        });
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ColumnSummary, ColumnType, DatasetSummary, Encoding};
    use chrono::Utc;
    use uuid::Uuid;

    // --- is_supported_dataset_extension ---

    #[test]
    fn extension_csv_lowercase() {
        assert!(is_supported_dataset_extension("data.csv"));
    }

    #[test]
    fn extension_csv_uppercase() {
        assert!(is_supported_dataset_extension("data.CSV"));
    }

    #[test]
    fn extension_csv_mixed_case() {
        assert!(is_supported_dataset_extension("data.CsV"));
    }

    #[test]
    fn extension_tsv() {
        assert!(is_supported_dataset_extension("file.tsv"));
    }

    #[test]
    fn extension_xlsx() {
        assert!(is_supported_dataset_extension("report.xlsx"));
    }

    #[test]
    fn extension_xls() {
        assert!(is_supported_dataset_extension("old.XLS"));
    }

    #[test]
    fn extension_unsupported_json() {
        assert!(!is_supported_dataset_extension("data.json"));
    }

    #[test]
    fn extension_unsupported_txt() {
        assert!(!is_supported_dataset_extension("notes.txt"));
    }

    #[test]
    fn extension_no_dot() {
        assert!(!is_supported_dataset_extension("csvfile"));
    }

    #[test]
    fn extension_empty_string() {
        assert!(!is_supported_dataset_extension(""));
    }

    #[test]
    fn extension_dot_only() {
        assert!(!is_supported_dataset_extension("."));
    }

    #[test]
    fn extension_multiple_dots() {
        assert!(is_supported_dataset_extension("my.data.file.csv"));
    }

    #[test]
    fn extension_hidden_file_csv() {
        assert!(is_supported_dataset_extension(".hidden.csv"));
    }

    // --- validate_dataset_size ---

    const MAX_SIZE: u64 = 50 * 1024 * 1024;
    const MAX_ROWS: u64 = 1_000_000;

    #[test]
    fn size_at_boundary_ok() {
        assert!(validate_dataset_size(MAX_SIZE, MAX_ROWS).is_ok());
    }

    #[test]
    fn size_zero_ok() {
        assert!(validate_dataset_size(0, 0).is_ok());
    }

    #[test]
    fn size_exceeds_by_one_byte() {
        let err = validate_dataset_size(MAX_SIZE + 1, 0).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::DatasetTooLarge);
        assert!(err.message.contains("数据文件过大"));
    }

    #[test]
    fn rows_exceed_by_one() {
        let err = validate_dataset_size(0, MAX_ROWS + 1).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::DatasetTooLarge);
        assert!(err.message.contains("数据文件过大"));
        assert!(err.message.contains("行"));
    }

    #[test]
    fn both_size_and_rows_exceed() {
        let err = validate_dataset_size(MAX_SIZE + 1, MAX_ROWS + 1).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::DatasetTooLarge);
    }

    // --- validate_dataset_non_empty ---

    fn make_summary(row_count: u64, col_count: usize) -> DatasetSummary {
        let columns: Vec<ColumnSummary> = (0..col_count)
            .map(|i| ColumnSummary {
                name: format!("col_{i}"),
                inferred_type: ColumnType::Numeric,
                missing_count: 0,
            })
            .collect();
        DatasetSummary {
            dataset_id: Uuid::new_v4(),
            file_name: "test.csv".to_string(),
            size_bytes: 100,
            encoding: Encoding::Utf8,
            row_count,
            columns,
            uploaded_at: Utc::now(),
        }
    }

    #[test]
    fn non_empty_valid() {
        let s = make_summary(10, 3);
        assert!(validate_dataset_non_empty(&s).is_ok());
    }

    #[test]
    fn non_empty_zero_columns() {
        let s = make_summary(10, 0);
        let err = validate_dataset_non_empty(&s).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::DatasetEmpty);
        assert!(err.message.contains("列"));
    }

    #[test]
    fn non_empty_zero_rows() {
        let s = make_summary(0, 3);
        let err = validate_dataset_non_empty(&s).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::DatasetEmpty);
        assert!(err.message.contains("行"));
    }

    #[test]
    fn non_empty_zero_both() {
        // When both are zero, columns check triggers first
        let s = make_summary(0, 0);
        let err = validate_dataset_non_empty(&s).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::DatasetEmpty);
    }

    // --- check_upload_quota ---

    const QUOTA_200MB: u64 = 200 * 1024 * 1024;

    #[test]
    fn quota_within_limit() {
        assert!(check_upload_quota(100 * 1024 * 1024, 50 * 1024 * 1024, QUOTA_200MB).is_ok());
    }

    #[test]
    fn quota_at_boundary_ok() {
        // used + new_size == quota → Ok
        assert!(check_upload_quota(150 * 1024 * 1024, 50 * 1024 * 1024, QUOTA_200MB).is_ok());
    }

    #[test]
    fn quota_exceeds_by_one() {
        let err = check_upload_quota(QUOTA_200MB, 1, QUOTA_200MB).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::SessionQuotaExceeded);
        assert!(err.message.contains("200 MB"));
    }

    #[test]
    fn quota_saturating_add_overflow() {
        // u64::MAX + 1 should saturate, not panic
        let err = check_upload_quota(u64::MAX, 1, QUOTA_200MB).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::SessionQuotaExceeded);
    }

    #[test]
    fn quota_zero_new_size_ok() {
        assert!(check_upload_quota(QUOTA_200MB, 0, QUOTA_200MB).is_ok());
    }

    #[test]
    fn quota_message_contains_dynamic_mb() {
        let quota = 500 * 1024 * 1024; // 500 MB
        let err = check_upload_quota(quota, 1, quota).unwrap_err();
        assert!(err.message.contains("500 MB"));
    }
}
