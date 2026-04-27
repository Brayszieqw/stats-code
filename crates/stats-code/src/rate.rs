// ---------------------------------------------------------------------------
// Rate (incidence rate / person-time rate) analysis module.
// ---------------------------------------------------------------------------

use std::collections::BTreeMap;
use std::path::Path;

use crate::cli::RateArgs;
use crate::helpers::{
    normalize_group_value_for_column, parse_event_value, parse_positive_weight, require_column,
    stringify_error,
};
use crate::math::poisson_rate_ci_per_1000;
use crate::schema::{is_missing_value_for_column, AnalysisSpec, RateResult, RateRow};

pub(crate) fn rate_csv(
    path: &Path,
    analysis_path: Option<&Path>,
    analysis_spec: Option<&AnalysisSpec>,
    args: &RateArgs,
) -> Result<RateResult, String> {
    let mut reader = csv::Reader::from_path(path).map_err(stringify_error)?;
    let headers = reader
        .headers()
        .map_err(stringify_error)?
        .iter()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let header_index = headers
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let event_index = require_column(&header_index, &args.event)?;
    let person_time_index = require_column(&header_index, &args.person_time)?;
    let survey_weight = analysis_spec
        .and_then(|spec| spec.survey.as_ref())
        .and_then(|survey| survey.weight.clone());
    let weight_index = survey_weight
        .as_ref()
        .map(|name| require_column(&header_index, name).map(|index| (name.clone(), index)))
        .transpose()?;
    let strata_indices = args
        .strata
        .iter()
        .map(|name| require_column(&header_index, name).map(|index| (name.clone(), index)))
        .collect::<Result<Vec<_>, _>>()?;

    let mut by_stratum = BTreeMap::<String, RateAccumulator>::new();
    let mut skipped_missing = 0usize;
    let mut skipped_invalid = 0usize;
    let mut skipped_missing_weight = 0usize;
    let mut skipped_invalid_weight = 0usize;

    for record in reader.records() {
        let record = record.map_err(stringify_error)?;
        let stratum = if strata_indices.is_empty() {
            "overall".to_string()
        } else {
            strata_indices
                .iter()
                .map(|(name, index)| {
                    format!(
                        "{}={}",
                        name,
                        normalize_group_value_for_column(
                            name,
                            record.get(*index).unwrap_or_default()
                        )
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        };

        let entry = by_stratum.entry(stratum).or_default();
        entry.total_records += 1;

        let event_raw = record.get(event_index).unwrap_or_default();
        let person_time_raw = record.get(person_time_index).unwrap_or_default();
        if is_missing_value_for_column(&args.event, event_raw)
            || is_missing_value_for_column(&args.person_time, person_time_raw)
        {
            skipped_missing += 1;
            continue;
        }

        let Some(events) = parse_event_value(event_raw) else {
            skipped_invalid += 1;
            continue;
        };
        let Ok(person_time) = person_time_raw.trim().parse::<f64>() else {
            skipped_invalid += 1;
            continue;
        };
        if !person_time.is_finite() || person_time <= 0.0 || !events.is_finite() || events < 0.0 {
            skipped_invalid += 1;
            continue;
        }
        let weight = if let Some((weight_name, weight_index)) = &weight_index {
            match parse_positive_weight(weight_name, record.get(*weight_index).unwrap_or_default())
            {
                Ok(Some(value)) => value,
                Ok(None) => {
                    skipped_missing_weight += 1;
                    continue;
                }
                Err(_) => {
                    skipped_invalid_weight += 1;
                    continue;
                }
            }
        } else {
            1.0
        };

        entry.included_records += 1;
        entry.events += events * weight;
        entry.person_time += person_time * weight;
    }

    let rows = by_stratum
        .into_iter()
        .map(|(stratum, accumulator)| {
            let rate = if accumulator.person_time > 0.0 {
                accumulator.events / accumulator.person_time
            } else {
                0.0
            };
            let (lower_ci_per_1000, upper_ci_per_1000) =
                poisson_rate_ci_per_1000(accumulator.events, accumulator.person_time);
            RateRow {
                stratum,
                total_records: accumulator.total_records,
                included_records: accumulator.included_records,
                events: accumulator.events,
                person_time: accumulator.person_time,
                rate,
                rate_per_1000: rate * 1000.0,
                lower_ci_per_1000,
                upper_ci_per_1000,
            }
        })
        .collect::<Vec<_>>();

    Ok(RateResult {
        status: "ok".to_string(),
        data_path: path.display().to_string(),
        analysis_path: analysis_path.map(|path| path.display().to_string()),
        event: args.event.clone(),
        person_time: args.person_time.clone(),
        strata: args.strata.clone(),
        survey_weight: survey_weight.clone(),
        rows,
        notes: rate_notes(
            &args.event,
            &args.person_time,
            survey_weight.as_deref(),
            skipped_missing,
            skipped_invalid,
            skipped_missing_weight,
            skipped_invalid_weight,
        ),
    })
}

fn rate_notes(
    event: &str,
    person_time: &str,
    survey_weight: Option<&str>,
    skipped_missing: usize,
    skipped_invalid: usize,
    skipped_missing_weight: usize,
    skipped_invalid_weight: usize,
) -> Vec<String> {
    let mut notes = vec![
        format!("Skipped {skipped_missing} rows with missing `{event}` or `{person_time}`."),
        format!(
            "Skipped {skipped_invalid} rows with invalid/non-positive event or person-time values."
        ),
        "Rates are reported per 1 person-time unit and per 1000 person-time units.".to_string(),
        "95% intervals use a Byar-style Poisson approximation on event counts.".to_string(),
    ];
    if let Some(weight) = survey_weight {
        notes.push(format!(
            "Survey weight `{weight}` was applied to event counts and person-time totals."
        ));
        notes.push(
            "Complex survey design variance is not applied to the Poisson interval.".to_string(),
        );
        notes.push(format!(
            "Rows excluded for missing `{weight}`: {skipped_missing_weight}; invalid/non-positive `{weight}`: {skipped_invalid_weight}."
        ));
    }
    notes
}

#[derive(Debug, Default)]
pub(crate) struct RateAccumulator {
    total_records: usize,
    included_records: usize,
    events: f64,
    person_time: f64,
}
