use std::fs;
use std::path::Path;

use crate::cli::{InspectArgs, RateArgs, TableOneArgs};
use crate::helpers::{excel_to_temp_csv, read_excel_records, stringify_error};
use crate::rate::rate_csv;
use crate::report::{ensure_study_context_ready, resolve_data_path};
use crate::schema::{
    detect_data_format, load_analysis_spec, DataFormat, InspectResult, RateResult,
    RunningColumnStats, TableOneResult,
};
use crate::tableone::tableone_csv;
pub(crate) fn handle_inspect(args: &InspectArgs) -> Result<InspectResult, String> {
    let format = detect_data_format(&args.data_path);
    match format {
        DataFormat::Csv => inspect_csv(&args.data_path),
        DataFormat::Excel => inspect_excel(&args.data_path),
        DataFormat::Parquet | DataFormat::Xpt => Ok(InspectResult {
            status: "unsupported".to_string(),
            data_path: args.data_path.display().to_string(),
            format,
            rows: None,
            columns: 0,
            variables: Vec::new(),
            notes: vec![format!(
                "{:?} format is not yet supported for inspect. \
                     Please convert your file to CSV first, for example: \
                     `pandas.read_excel('file.xlsx').to_csv('file.csv', index=False)`",
                format
            )],
        }),
        DataFormat::Unknown => Err(format!(
            "Unsupported data file extension for `{}`. Expected csv, xlsx/xls, parquet, or xpt.",
            args.data_path.display()
        )),
    }
}

fn inspect_csv(path: &Path) -> Result<InspectResult, String> {
    let mut reader = csv::Reader::from_path(path).map_err(stringify_error)?;
    let headers = reader
        .headers()
        .map_err(stringify_error)?
        .iter()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let mut stats = headers
        .iter()
        .map(|name| RunningColumnStats::new(name))
        .collect::<Vec<_>>();
    let mut rows = 0usize;

    for record in reader.records() {
        let record = record.map_err(stringify_error)?;
        rows += 1;
        for (index, value) in record.iter().enumerate() {
            if let Some(stat) = stats.get_mut(index) {
                stat.observe(value);
            }
        }
    }

    let variables = stats
        .into_iter()
        .map(RunningColumnStats::finish)
        .collect::<Vec<_>>();
    let high_missing_columns = variables
        .iter()
        .filter(|column| column.missing_count > 0)
        .count();

    Ok(InspectResult {
        status: "ok".to_string(),
        data_path: path.display().to_string(),
        format: DataFormat::Csv,
        rows: Some(rows),
        columns: headers.len(),
        variables,
        notes: vec![
            "CSV inspection is deterministic and local.".to_string(),
            "Missing values detected: blank, NA, N/A, null, missing, none, unknown, ., -, nd, nm, 9/99/999/9999.".to_string(),
            format!("Columns with at least one missing value: {high_missing_columns}."),
        ],
    })
}

fn inspect_excel(path: &Path) -> Result<InspectResult, String> {
    let (headers, records) = read_excel_records(path)?;
    let mut stats = headers
        .iter()
        .map(|name| RunningColumnStats::new(name))
        .collect::<Vec<_>>();
    let rows = records.len();

    for record in &records {
        for (index, value) in record.iter().enumerate() {
            if let Some(stat) = stats.get_mut(index) {
                stat.observe(value);
            }
        }
    }

    let variables = stats
        .into_iter()
        .map(RunningColumnStats::finish)
        .collect::<Vec<_>>();
    let high_missing_columns = variables
        .iter()
        .filter(|column| column.missing_count > 0)
        .count();

    Ok(InspectResult {
        status: "ok".to_string(),
        data_path: path.display().to_string(),
        format: DataFormat::Excel,
        rows: Some(rows),
        columns: headers.len(),
        variables,
        notes: vec![
            "Excel inspection reads the first worksheet.".to_string(),
            "Missing values detected: blank, NA, N/A, null, missing, none, unknown, ., -, nd, nm, 9/99/999/9999.".to_string(),
            format!("Columns with at least one missing value: {high_missing_columns}."),
        ],
    })
}

pub(crate) fn handle_tableone(args: &TableOneArgs) -> Result<TableOneResult, String> {
    let (data_path, analysis_path) = resolve_data_path(args.data.as_ref(), args.analysis.as_ref())?;
    let analysis_spec = analysis_path
        .as_ref()
        .map(|path| load_analysis_spec(path))
        .transpose()?;
    if let (Some(path), Some(spec)) = (analysis_path.as_deref(), analysis_spec.as_ref()) {
        ensure_study_context_ready(path, spec)?;
    }
    match detect_data_format(&data_path) {
        DataFormat::Csv => tableone_csv(
            &data_path,
            analysis_path.as_deref(),
            analysis_spec.as_ref(),
            args,
        ),
        DataFormat::Excel => {
            let tmp = excel_to_temp_csv(&data_path)?;
            let result = tableone_csv(&tmp, analysis_path.as_deref(), analysis_spec.as_ref(), args);
            let _ = fs::remove_file(&tmp);
            result
        }
        format => Err(format!(
            "Unsupported format `{:?}` for `{}`. Supported: CSV, Excel (xls/xlsx).",
            format,
            data_path.display()
        )),
    }
}

pub(crate) fn handle_rate(args: &RateArgs) -> Result<RateResult, String> {
    let (data_path, analysis_path) = resolve_data_path(args.data.as_ref(), args.analysis.as_ref())?;
    let analysis_spec = analysis_path
        .as_ref()
        .map(|path| load_analysis_spec(path))
        .transpose()?;
    if let (Some(path), Some(spec)) = (analysis_path.as_deref(), analysis_spec.as_ref()) {
        ensure_study_context_ready(path, spec)?;
    }
    match detect_data_format(&data_path) {
        DataFormat::Csv => rate_csv(
            &data_path,
            analysis_path.as_deref(),
            analysis_spec.as_ref(),
            args,
        ),
        DataFormat::Excel => {
            let tmp = excel_to_temp_csv(&data_path)?;
            let result = rate_csv(&tmp, analysis_path.as_deref(), analysis_spec.as_ref(), args);
            let _ = fs::remove_file(&tmp);
            result
        }
        format => Err(format!(
            "Unsupported format `{:?}` for `{}`. Supported: CSV, Excel (xls/xlsx).",
            format,
            data_path.display()
        )),
    }
}
