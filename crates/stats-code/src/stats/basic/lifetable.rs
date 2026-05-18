use crate::cli::NaStrategy;
use crate::helpers::require_column;
use crate::schema::{LifeTableResult, LifeTableRow};

use super::common::*;

pub(crate) fn lifetable_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    intervals_col: &str,
    entering_col: &str,
    events_col: &str,
    withdrawals_col: &str,
    alpha: f64,
    strategy: NaStrategy,
) -> Result<LifeTableResult, String> {
    let index = column_index(headers);
    let ii = require_column(&index, intervals_col)?;
    let ien = require_column(&index, entering_col)?;
    let iev = require_column(&index, events_col)?;
    let iw = require_column(&index, withdrawals_col)?;
    let mut out = Vec::new();
    let mut excluded = 0usize;
    let mut survival = 1.0;
    let mut cumulative_hazard = 0.0;
    let mut greenwood = 0.0;
    let z = z_critical(alpha);
    for row in rows {
        let ri = row.get(ii).unwrap_or("").trim();
        let ren = row.get(ien).unwrap_or("").trim();
        let rev = row.get(iev).unwrap_or("").trim();
        let rw = row.get(iw).unwrap_or("").trim();
        if missing(intervals_col, ri)
            || missing(entering_col, ren)
            || missing(events_col, rev)
            || missing(withdrawals_col, rw)
        {
            excluded += 1;
            continue;
        }
        let entering = parse_num(ren, entering_col)?.round().max(0.0) as usize;
        let events = parse_num(rev, events_col)?.round().max(0.0) as usize;
        let withdrawals = parse_num(rw, withdrawals_col)?.round().max(0.0) as usize;
        let effective = entering as f64 - withdrawals as f64 / 2.0;
        let conditional = if effective > 0.0 {
            (1.0 - events as f64 / effective).clamp(0.0, 1.0)
        } else {
            1.0
        };
        survival *= conditional;
        if effective > events as f64 {
            greenwood += events as f64 / (effective * (effective - events as f64)).max(EPS);
        }
        let se = survival * greenwood.sqrt();
        let (start, end) = parse_interval(ri, out.len() as f64);
        let width = (end - start).abs().max(EPS);
        let hazard = events as f64 / effective.max(EPS) / width;
        cumulative_hazard += hazard * width;
        out.push(LifeTableRow {
            interval_index: out.len(),
            start,
            end,
            entering,
            withdrawals,
            events,
            effective_at_risk: effective,
            conditional_survival: conditional,
            cumulative_survival: survival,
            se_cumulative: se,
            ci_lower: (survival - z * se).max(0.0),
            ci_upper: (survival + z * se).min(1.0),
            hazard_rate: hazard,
            cumulative_hazard,
        });
    }
    check_missing_policy(excluded, strategy, "life table")?;
    Ok(LifeTableResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: out.len(),
        n_excluded_missing: excluded,
        notes: prelude_notes(out.len(), rows.len(), excluded),
        warnings: vec![],
        time: intervals_col.to_string(),
        intervals: out,
    })
}

pub(crate) fn lifetable_individual_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    time_col: &str,
    status_col: &str,
    interval_spec: &str,
    alpha: f64,
    strategy: NaStrategy,
) -> Result<LifeTableResult, String> {
    let index = column_index(headers);
    let itime = require_column(&index, time_col)?;
    let istatus = require_column(&index, status_col)?;
    let mut observations = Vec::new();
    let mut excluded = 0usize;
    for row in rows {
        let raw_time = row.get(itime).unwrap_or("").trim();
        let raw_status = row.get(istatus).unwrap_or("").trim();
        if missing(time_col, raw_time) || missing(status_col, raw_status) {
            excluded += 1;
            continue;
        }
        let time = parse_num(raw_time, time_col)?;
        let Some(event) = event_value(raw_status, status_col, None) else {
            excluded += 1;
            continue;
        };
        if time < 0.0 {
            return Err("Life table individual input requires non-negative times.".to_string());
        }
        observations.push((time, event));
    }
    check_missing_policy(excluded, strategy, "life table individual input")?;
    if observations.is_empty() {
        return Err(
            "Life table individual input requires at least one complete observation.".to_string(),
        );
    }
    let intervals = individual_intervals(interval_spec, &observations)?;
    let z = z_critical(alpha);
    let mut out = Vec::new();
    let mut survival = 1.0;
    let mut cumulative_hazard = 0.0;
    let mut greenwood = 0.0;
    for (idx, (start, end)) in intervals.iter().enumerate() {
        let is_last = idx + 1 == intervals.len();
        let entering = observations
            .iter()
            .filter(|(time, _)| *time >= *start)
            .count();
        let events = observations
            .iter()
            .filter(|(time, event)| {
                *event && *time >= *start && (*time < *end || (is_last && *time <= *end))
            })
            .count();
        let withdrawals = observations
            .iter()
            .filter(|(time, event)| {
                !*event && *time >= *start && (*time < *end || (is_last && *time <= *end))
            })
            .count();
        let effective = entering as f64 - withdrawals as f64 / 2.0;
        let conditional = if effective > 0.0 {
            (1.0 - events as f64 / effective).clamp(0.0, 1.0)
        } else {
            1.0
        };
        survival *= conditional;
        if effective > events as f64 {
            greenwood += events as f64 / (effective * (effective - events as f64)).max(EPS);
        }
        let se = survival * greenwood.sqrt();
        let width = (end - start).abs().max(EPS);
        let hazard = events as f64 / effective.max(EPS) / width;
        cumulative_hazard += hazard * width;
        out.push(LifeTableRow {
            interval_index: idx,
            start: *start,
            end: *end,
            entering,
            withdrawals,
            events,
            effective_at_risk: effective,
            conditional_survival: conditional,
            cumulative_survival: survival,
            se_cumulative: se,
            ci_lower: (survival - z * se).max(0.0),
            ci_upper: (survival + z * se).min(1.0),
            hazard_rate: hazard,
            cumulative_hazard,
        });
    }
    Ok(LifeTableResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: observations.len(),
        n_excluded_missing: excluded,
        notes: prelude_notes(observations.len(), rows.len(), excluded),
        warnings: vec![],
        time: time_col.to_string(),
        intervals: out,
    })
}

fn individual_intervals(
    spec: &str,
    observations: &[(f64, bool)],
) -> Result<Vec<(f64, f64)>, String> {
    if let Some(width_raw) = spec.strip_prefix("width=") {
        let width = width_raw.trim().parse::<f64>().map_err(|_| {
            "Life table width specification must be numeric, e.g. width=1.".to_string()
        })?;
        if width <= 0.0 {
            return Err("Life table interval width must be positive.".to_string());
        }
        let max_time = observations
            .iter()
            .map(|(time, _)| *time)
            .fold(0.0_f64, f64::max);
        let upper = (max_time / width).ceil().max(1.0) * width;
        let mut intervals = Vec::new();
        let mut start = 0.0;
        while start < upper {
            intervals.push((start, start + width));
            start += width;
        }
        return Ok(intervals);
    }
    let bounds = spec
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| {
            item.parse::<f64>()
                .map_err(|_| format!("Invalid life table interval boundary `{item}`."))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if bounds.len() < 2 {
        return Err("Life table individual intervals require comma boundaries such as 0,1,2,5 or width=<positive>.".to_string());
    }
    if bounds.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("Life table interval boundaries must be strictly increasing.".to_string());
    }
    Ok(bounds.windows(2).map(|pair| (pair[0], pair[1])).collect())
}

fn parse_interval(raw: &str, fallback: f64) -> (f64, f64) {
    let normalized = raw
        .replace('[', "")
        .replace(']', "")
        .replace('(', "")
        .replace(')', "");
    for sep in ["-", ",", ".."] {
        if let Some((a, b)) = normalized.split_once(sep) {
            if let (Ok(start), Ok(end)) = (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
                return (start, end);
            }
        }
    }
    if let Ok(start) = raw.parse::<f64>() {
        (start, start + 1.0)
    } else {
        (fallback, fallback + 1.0)
    }
}
