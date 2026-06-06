//! Dataset domain model.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::session::SessionId;

/// Type alias for dataset identifiers.
pub type DatasetId = Uuid;

/// Reference to a raw uploaded dataset file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetRef {
    pub session_id: SessionId,
    pub dataset_id: DatasetId,
    pub raw_path: PathBuf,
}

/// Detected file encoding (R3.6: detection order UTF-8 → GBK → UTF-16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Encoding {
    Utf8,
    Gbk,
    Utf16,
}

/// Inferred column data type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnType {
    Numeric,
    Categorical,
    Date,
    String,
}

/// Summary of a single column in a dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSummary {
    pub name: String,
    pub inferred_type: ColumnType,
    pub missing_count: u64,
}

/// Parsed summary of an uploaded dataset (R3.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetSummary {
    pub dataset_id: DatasetId,
    pub file_name: String,
    pub size_bytes: u64,
    pub encoding: Encoding,
    pub row_count: u64,
    pub columns: Vec<ColumnSummary>,
    pub uploaded_at: DateTime<Utc>,
    /// 64 lowercase hex SHA256 of the exact raw upload bytes. `None` for a
    /// legacy summary persisted before this field existed (Requirement 1.7);
    /// always serialized (as a JSON string or `null`) so present-vs-absent is
    /// distinguishable on the wire (Requirement 1.3, 1.8).
    #[serde(default)]
    pub sha256: Option<String>,
}
