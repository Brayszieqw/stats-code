use std::collections::BTreeSet;
use std::path::Path;

use crate::cli::NaStrategy;
use crate::helpers::require_column;
use crate::schema::{PsmCovariateSmd, PsmResult};

use super::common::*;

pub(crate) fn psm_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    treatment_col: &str,
    covariates: &[String],
    caliper: f64,
    ratio: usize,
    seed: Option<u64>,
    strategy: NaStrategy,
    output_path: Option<&Path>,
) -> Result<PsmResult, String> {
    let _seed = seed.ok_or_else(|| "PSM requires --seed for reproducibility.".to_string())?;
    if ratio == 0 {
        return Err("PSM --ratio must be at least 1.".to_string());
    }
    let index = column_index(headers);
    let it = require_column(&index, treatment_col)?;
    let cov_idx = covariates
        .iter()
        .map(|c| require_column(&index, c).map(|idx| (c.clone(), idx)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut t = Vec::new();
    let mut x = Vec::new();
    let mut complete_row_indices = Vec::new();
    let mut excluded = 0usize;
    for (row_index, row) in rows.iter().enumerate() {
        let rt = row.get(it).unwrap_or("").trim();
        let Some(treated) = event_value(rt, treatment_col, None) else {
            excluded += 1;
            continue;
        };
        let mut covs = Vec::new();
        let mut bad = false;
        for (name, idx) in &cov_idx {
            let raw = row.get(*idx).unwrap_or("").trim();
            if missing(name, raw) {
                bad = true;
                break;
            }
            covs.push(parse_num(raw, name)?);
        }
        if bad {
            excluded += 1;
        } else {
            t.push(treated);
            x.push(covs);
            complete_row_indices.push(row_index);
        }
    }
    check_missing_policy(excluded, strategy, "PSM")?;
    let scores = simple_propensity_scores(&t, &x);
    let sd_score = sample_sd(&scores).max(EPS);
    let threshold = caliper * sd_score;
    let treated_indices: Vec<usize> = t
        .iter()
        .enumerate()
        .filter_map(|(i, v)| if *v { Some(i) } else { None })
        .collect();
    let control_indices: Vec<usize> = t
        .iter()
        .enumerate()
        .filter_map(|(i, v)| if !*v { Some(i) } else { None })
        .collect();
    let mut used_controls = BTreeSet::new();
    let mut matched_complete_indices = BTreeSet::new();
    let mut matched_sets = Vec::new();
    let mut matched_pairs = 0usize;
    for (set_index, ti) in treated_indices.iter().enumerate() {
        let mut candidates = control_indices
            .iter()
            .filter(|ci| !used_controls.contains(*ci))
            .map(|ci| (*ci, (scores[*ti] - scores[*ci]).abs()))
            .filter(|(_, d)| *d <= threshold)
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut controls_for_set = Vec::new();
        for (ci, _) in candidates.into_iter().take(ratio) {
            if used_controls.insert(ci) {
                matched_pairs += 1;
                matched_complete_indices.insert(ci);
                controls_for_set.push(ci);
            }
        }
        if !controls_for_set.is_empty() {
            matched_complete_indices.insert(*ti);
            matched_sets.push((set_index + 1, *ti, controls_for_set));
        }
    }
    let matched_t = matched_complete_indices
        .iter()
        .map(|idx| t[*idx])
        .collect::<Vec<_>>();
    let matched_x = matched_complete_indices
        .iter()
        .map(|idx| x[*idx].clone())
        .collect::<Vec<_>>();
    let balance = covariates
        .iter()
        .enumerate()
        .map(|(j, name)| {
            let before = smd_for_covariate(&t, &x, j);
            let after = if matched_x.is_empty() {
                f64::NAN
            } else {
                smd_for_covariate(&matched_t, &matched_x, j)
            };
            PsmCovariateSmd {
                covariate: name.clone(),
                smd_before: before,
                smd_after: after,
            }
        })
        .collect();
    let matched_dataset_path = if let Some(path) = output_path {
        write_psm_matched_csv(
            path,
            headers,
            rows,
            &complete_row_indices,
            &scores,
            &matched_sets,
        )?;
        path.display().to_string()
    } else {
        String::new()
    };
    Ok(PsmResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: t.len(),
        n_excluded_missing: excluded,
        notes: prelude_notes(t.len(), rows.len(), excluded),
        warnings: vec![],
        treatment: treatment_col.to_string(),
        covariates: covariates.to_vec(),
        caliper,
        ratio,
        n_treated: treated_indices.len(),
        n_control: control_indices.len(),
        n_matched_pairs: matched_pairs,
        n_unmatched_treated: treated_indices.len().saturating_sub(matched_pairs),
        n_unmatched_control: control_indices.len().saturating_sub(used_controls.len()),
        balance,
        matched_dataset_path,
    })
}

fn write_psm_matched_csv(
    path: &Path,
    headers: &csv::StringRecord,
    rows: &[csv::StringRecord],
    complete_row_indices: &[usize],
    scores: &[f64],
    matched_sets: &[(usize, usize, Vec<usize>)],
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Cannot create PSM output directory `{}`: {error}",
                    parent.display()
                )
            })?;
        }
    }
    let mut writer = csv::Writer::from_path(path).map_err(|error| {
        format!(
            "Cannot create PSM matched dataset `{}`: {error}",
            path.display()
        )
    })?;
    let mut out_headers = headers.clone();
    out_headers.push_field("psm_match_set");
    out_headers.push_field("psm_role");
    out_headers.push_field("psm_propensity_score");
    writer
        .write_record(&out_headers)
        .map_err(stringify_csv_error)?;
    for (set_id, treated_idx, controls) in matched_sets {
        write_psm_row(
            &mut writer,
            rows,
            complete_row_indices,
            scores,
            *treated_idx,
            *set_id,
            "treated",
        )?;
        for control_idx in controls {
            write_psm_row(
                &mut writer,
                rows,
                complete_row_indices,
                scores,
                *control_idx,
                *set_id,
                "control",
            )?;
        }
    }
    writer.flush().map_err(stringify_csv_error)
}

fn write_psm_row(
    writer: &mut csv::Writer<std::fs::File>,
    rows: &[csv::StringRecord],
    complete_row_indices: &[usize],
    scores: &[f64],
    complete_idx: usize,
    set_id: usize,
    role: &str,
) -> Result<(), String> {
    let source_idx = complete_row_indices
        .get(complete_idx)
        .copied()
        .ok_or_else(|| "Internal PSM row index was out of bounds.".to_string())?;
    let mut record = rows
        .get(source_idx)
        .cloned()
        .ok_or_else(|| "Internal PSM source row index was out of bounds.".to_string())?;
    record.push_field(&set_id.to_string());
    record.push_field(role);
    record.push_field(&format!(
        "{:.12}",
        scores.get(complete_idx).copied().unwrap_or(f64::NAN)
    ));
    writer.write_record(&record).map_err(stringify_csv_error)
}

fn stringify_csv_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn simple_propensity_scores(treatment: &[bool], x: &[Vec<f64>]) -> Vec<f64> {
    if x.is_empty() {
        return Vec::new();
    }
    let p = x[0].len();
    let mut scores = vec![0.0; x.len()];
    for j in 0..p {
        let col: Vec<f64> = x.iter().map(|row| row[j]).collect();
        let m = mean(&col);
        let sd = sample_sd(&col).max(EPS);
        let mt = mean(
            &x.iter()
                .zip(treatment)
                .filter_map(|(row, t)| if *t { Some(row[j]) } else { None })
                .collect::<Vec<_>>(),
        );
        let mc = mean(
            &x.iter()
                .zip(treatment)
                .filter_map(|(row, t)| if !*t { Some(row[j]) } else { None })
                .collect::<Vec<_>>(),
        );
        let direction = (mt - mc).signum();
        for i in 0..x.len() {
            scores[i] += direction * (x[i][j] - m) / sd;
        }
    }
    scores
}

fn smd_for_covariate(treatment: &[bool], x: &[Vec<f64>], j: usize) -> f64 {
    let treated: Vec<f64> = x
        .iter()
        .zip(treatment)
        .filter_map(|(row, t)| if *t { Some(row[j]) } else { None })
        .collect();
    let control: Vec<f64> = x
        .iter()
        .zip(treatment)
        .filter_map(|(row, t)| if !*t { Some(row[j]) } else { None })
        .collect();
    if treated.is_empty() || control.is_empty() {
        return f64::NAN;
    }
    let pooled = ((sample_variance(&treated) + sample_variance(&control)) / 2.0)
        .sqrt()
        .max(EPS);
    (mean(&treated) - mean(&control)) / pooled
}
