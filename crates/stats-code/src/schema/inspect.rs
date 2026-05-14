use std::collections::BTreeSet;
use std::path::Path;

use super::contract::AnalysisSpec;
use super::results::{ColumnInspection, NumericSummary};
use super::types::{DataFormat, VariableKind};

pub struct RunningColumnStats {
    name: String,
    missing_count: usize,
    non_missing_count: usize,
    numeric_non_missing_count: usize,
    numeric_sum: f64,
    numeric_min: Option<f64>,
    numeric_max: Option<f64>,
    zero_count: usize,
    distinct_values: BTreeSet<String>,
    sample_values: Vec<String>,
}

impl RunningColumnStats {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            missing_count: 0,
            non_missing_count: 0,
            numeric_non_missing_count: 0,
            numeric_sum: 0.0,
            numeric_min: None,
            numeric_max: None,
            zero_count: 0,
            distinct_values: BTreeSet::new(),
            sample_values: Vec::new(),
        }
    }

    pub fn observe(&mut self, raw: &str) {
        let trimmed = raw.trim();
        if is_missing_value_for_column(&self.name, trimmed) {
            self.missing_count += 1;
            return;
        }
        self.non_missing_count += 1;
        if let Ok(value) = trimmed.parse::<f64>() {
            self.numeric_non_missing_count += 1;
            self.numeric_sum += value;
            self.numeric_min = Some(self.numeric_min.map_or(value, |current| current.min(value)));
            self.numeric_max = Some(self.numeric_max.map_or(value, |current| current.max(value)));
            if value == 0.0 {
                self.zero_count += 1;
            }
        }
        if self.distinct_values.len() < 128 {
            self.distinct_values.insert(trimmed.to_string());
        }
        if self.sample_values.len() < 5 && !self.sample_values.iter().any(|value| value == trimmed)
        {
            self.sample_values.push(trimmed.to_string());
        }
    }

    pub fn finish(self) -> ColumnInspection {
        let inferred_kind = infer_variable_kind(
            &self.name,
            self.non_missing_count,
            self.numeric_non_missing_count,
            &self.distinct_values,
        );
        let total_count = self.non_missing_count + self.missing_count;
        let missing_fraction = if total_count == 0 {
            0.0
        } else {
            self.missing_count as f64 / total_count as f64
        };
        let mut warnings = Vec::new();
        if missing_fraction >= 0.2 {
            warnings.push(format!("high_missingness={:.1}%", missing_fraction * 100.0));
        }
        if matches!(inferred_kind, VariableKind::Identifier)
            && self.non_missing_count > 0
            && self.distinct_values.len() == self.non_missing_count
        {
            warnings.push("possible_direct_identifier".to_string());
        }
        if matches!(inferred_kind, VariableKind::Continuous)
            && self.non_missing_count > 0
            && self.numeric_min == self.numeric_max
        {
            warnings.push("single_observed_value".to_string());
        }

        ColumnInspection {
            name: self.name.clone(),
            inferred_kind,
            missing_count: self.missing_count,
            non_missing_count: self.non_missing_count,
            distinct_count: self.distinct_values.len(),
            sample_values: self.sample_values,
            numeric_summary: if self.numeric_non_missing_count > 0 {
                Some(NumericSummary {
                    min: self.numeric_min.unwrap_or(0.0),
                    max: self.numeric_max.unwrap_or(0.0),
                    mean: self.numeric_sum / self.numeric_non_missing_count as f64,
                    zero_count: self.zero_count,
                })
            } else {
                None
            },
            warnings,
        }
    }
}

pub fn load_analysis_spec(path: &Path) -> Result<AnalysisSpec, String> {
    let contents = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let contents = contents.strip_prefix('\u{feff}').unwrap_or(&contents);
    serde_yaml::from_str(contents).map_err(|error| error.to_string())
}

pub fn detect_data_format(path: &Path) -> DataFormat {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("csv") => DataFormat::Csv,
        Some("xls" | "xlsx") => DataFormat::Excel,
        Some("parquet") => DataFormat::Parquet,
        Some("xpt") => DataFormat::Xpt,
        _ => DataFormat::Unknown,
    }
}

pub fn format_variable_kind(kind: VariableKind) -> &'static str {
    match kind {
        VariableKind::Continuous => "continuous",
        VariableKind::Categorical => "categorical",
        VariableKind::Ordered => "ordered",
        VariableKind::Binary => "binary",
        VariableKind::Time => "time",
        VariableKind::Date => "date",
        VariableKind::PersonTime => "person_time",
        VariableKind::Event => "event",
        VariableKind::Identifier => "identifier",
    }
}

pub fn infer_variable_kind(
    name: &str,
    non_missing_count: usize,
    numeric_non_missing_count: usize,
    distinct_values: &BTreeSet<String>,
) -> VariableKind {
    let lower = name.to_ascii_lowercase();
    // Person-time: explicit marker columns
    if lower.contains("person_time") || lower.ends_with("_pt") || lower.contains("fu_pt") {
        return VariableKind::PersonTime;
    }
    // Event/outcome: use precise patterns to avoid false positives like case_id, test_case
    if lower == "event"
        || lower == "death"
        || lower == "died"
        || lower == "outcome"
        || lower.ends_with("_event")
        || lower.ends_with("_death")
        || lower.ends_with("_died")
        || lower.starts_with("ev_")
        || lower.starts_with("event_")
    {
        return VariableKind::Event;
    }
    if lower.contains("date") || lower.ends_with("_dt") || lower.starts_with("dt_") {
        return VariableKind::Date;
    }
    if lower.contains("time") || lower.starts_with("fu_") || lower.ends_with("_time") {
        return VariableKind::Time;
    }
    if lower == "id" || lower.ends_with("_id") || lower.starts_with("id_") {
        return VariableKind::Identifier;
    }
    if non_missing_count > 0 && numeric_non_missing_count == non_missing_count {
        if distinct_values.len() <= 2 {
            return VariableKind::Binary;
        }
        return VariableKind::Continuous;
    }
    if distinct_values.len() <= 2 {
        return VariableKind::Binary;
    }
    if distinct_values.len() <= 8 {
        return VariableKind::Categorical;
    }
    VariableKind::Ordered
}

pub fn is_missing_value(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    let lower = value.to_ascii_lowercase();
    // Common text missing codes
    if matches!(
        lower.as_str(),
        "na" | "n/a"
            | "null"
            | "missing"
            | "none"
            | "unknown"
            | "."
            | "-"
            | "nd"
            | "nm"
            | "not applicable"
            | "not available"
            | "nan"
            | "inf"
            | "-inf"
    ) {
        return true;
    }
    // SAS-style sentinel values (common in epidemiology/clinical data)
    if matches!(value, "9" | "99" | "999" | "9999" | "99999" | "999999") {
        return true;
    }
    false
}

pub fn is_missing_value_for_column(column_name: &str, value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    if matches!(lower.as_str(), "nd" | "nm") && is_code_like_column(column_name) {
        return false;
    }
    is_missing_value(trimmed)
}

fn is_code_like_column(column_name: &str) -> bool {
    let compact = column_name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();

    matches!(
        compact.as_str(),
        "stateabbr" | "stateabbrev" | "statecode" | "geoid" | "locationid" | "countyfips"
    ) || compact.ends_with("abbr")
        || compact.ends_with("code")
        || compact.ends_with("fips")
        || compact.ends_with("id")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        is_missing_value, is_missing_value_for_column, load_analysis_spec, RunningColumnStats,
    };

    #[test]
    fn contextual_missing_value_preserves_state_codes() {
        assert!(is_missing_value("ND"));
        assert!(is_missing_value("NM"));
        assert!(!is_missing_value_for_column("stateabbr", "ND"));
        assert!(!is_missing_value_for_column("stateabbr", "NM"));
        assert!(!is_missing_value_for_column("measureid", "ND"));
        assert!(is_missing_value_for_column("lab_result", "ND"));
    }

    #[test]
    fn running_column_stats_uses_column_context_for_missing_codes() {
        let mut state_stats = RunningColumnStats::new("stateabbr");
        state_stats.observe("ND");
        state_stats.observe("NM");
        let state_column = state_stats.finish();
        assert_eq!(state_column.missing_count, 0);
        assert_eq!(state_column.non_missing_count, 2);
        assert_eq!(state_column.distinct_count, 2);

        let mut result_stats = RunningColumnStats::new("result");
        result_stats.observe("ND");
        let result_column = result_stats.finish();
        assert_eq!(result_column.missing_count, 1);
        assert_eq!(result_column.non_missing_count, 0);
    }

    #[test]
    fn analysis_yaml_loader_accepts_utf8_bom() {
        let root = std::env::temp_dir().join(format!(
            "stats-code-bom-analysis-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time after epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp dir");
        let path = root.join("analysis.yaml");
        let yaml = r"
study:
  title: BOM demo
  design: cohort
data:
  path: demo.csv
  format: csv
variables: []
analyses: []
";
        let mut bytes = vec![0xef, 0xbb, 0xbf];
        bytes.extend_from_slice(yaml.trim_start().as_bytes());
        fs::write(&path, bytes).expect("write bom yaml");

        let spec = load_analysis_spec(&path).expect("load bom yaml");
        assert_eq!(spec.study.title, "BOM demo");

        fs::remove_dir_all(root).expect("cleanup");
    }
}
