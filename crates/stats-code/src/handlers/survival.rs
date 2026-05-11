use std::fs;

use crate::cli::SurvivalKmArgs;
use crate::helpers::excel_to_temp_csv;
use crate::report::{ensure_study_context_ready, resolve_data_path};
use crate::schema::{detect_data_format, load_analysis_spec, DataFormat, SurvivalKmResult};
use crate::survival::survival_km_csv;

pub(crate) fn handle_survival_km(args: &SurvivalKmArgs) -> Result<SurvivalKmResult, String> {
    let (data_path, analysis_path) = resolve_data_path(args.data.as_ref(), args.analysis.as_ref())?;
    let analysis_spec = analysis_path
        .as_ref()
        .map(|path| load_analysis_spec(path))
        .transpose()?;
    if let (Some(path), Some(spec)) = (analysis_path.as_deref(), analysis_spec.as_ref()) {
        ensure_study_context_ready(path, spec)?;
    }
    match detect_data_format(&data_path) {
        DataFormat::Csv => survival_km_csv(&data_path, analysis_path.as_deref(), args),
        DataFormat::Excel => {
            let tmp = excel_to_temp_csv(&data_path)?;
            let result = survival_km_csv(&tmp, analysis_path.as_deref(), args);
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
