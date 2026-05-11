use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::cli::SurvivalKmArgs;
use crate::helpers::{require_column, stringify_error};
use crate::math::chi_square_cdf;
use crate::schema::{LogRankResult, SurvivalKmResult, SurvivalKmStep};

#[derive(Debug, Clone)]
struct SurvivalRecord {
    time: f64,
    event: bool,
    group: String,
}

pub fn survival_km_csv(
    path: &Path,
    analysis_path: Option<&Path>,
    args: &SurvivalKmArgs,
) -> Result<SurvivalKmResult, String> {
    let mut reader = csv::Reader::from_path(path).map_err(stringify_error)?;
    let headers = reader.headers().map_err(stringify_error)?.clone();
    let index: BTreeMap<String, usize> = headers
        .iter()
        .enumerate()
        .map(|(i, name)| (name.to_string(), i))
        .collect();
    let time_idx = require_column(&index, &args.time)?;
    let event_idx = require_column(&index, &args.event)?;
    let group_idx = args
        .group
        .as_ref()
        .map(|group| require_column(&index, group))
        .transpose()?;

    let mut n_total = 0usize;
    let mut n_excluded_missing = 0usize;
    let mut n_excluded_invalid = 0usize;
    let mut records = Vec::new();

    for record in reader.records() {
        n_total += 1;
        let record = record.map_err(stringify_error)?;
        let time_raw = record.get(time_idx).unwrap_or("").trim();
        let event_raw = record.get(event_idx).unwrap_or("").trim();
        let group_raw = group_idx
            .and_then(|idx| record.get(idx))
            .map_or("overall", str::trim);

        if time_raw.is_empty() || event_raw.is_empty() || group_raw.is_empty() {
            n_excluded_missing += 1;
            continue;
        }

        let Ok(time) = time_raw.parse::<f64>() else {
            n_excluded_invalid += 1;
            continue;
        };
        if !time.is_finite() || time < 0.0 {
            n_excluded_invalid += 1;
            continue;
        }

        let Some(event) = parse_binary_event(event_raw) else {
            n_excluded_invalid += 1;
            continue;
        };

        records.push(SurvivalRecord {
            time,
            event,
            group: group_raw.to_string(),
        });
    }

    if records.is_empty() {
        return Err("Kaplan-Meier analysis has no usable records after exclusions.".to_string());
    }

    let groups = records
        .iter()
        .map(|record| record.group.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let steps = kaplan_meier_steps(&records, &groups);
    let log_rank = if groups.len() >= 2 {
        Some(log_rank_test(&records, &groups))
    } else {
        None
    };

    Ok(SurvivalKmResult {
        status: "ok".to_string(),
        data_path: path.display().to_string(),
        analysis_path: analysis_path.map(|path| path.display().to_string()),
        time: args.time.clone(),
        event: args.event.clone(),
        group: args.group.clone(),
        n_total,
        n_used: records.len(),
        n_excluded_missing,
        n_excluded_invalid,
        groups,
        steps,
        log_rank,
        notes: vec!["Kaplan-Meier survival uses right-censoring and event indicators coded as 1/0, true/false, or yes/no.".to_string()],
        warnings: Vec::new(),
    })
}

fn parse_binary_event(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "event" | "dead" | "death" => Some(true),
        "0" | "false" | "no" | "n" | "censored" | "alive" => Some(false),
        value => value.parse::<f64>().ok().and_then(|number| {
            if (number - 1.0).abs() < f64::EPSILON {
                Some(true)
            } else if number.abs() < f64::EPSILON {
                Some(false)
            } else {
                None
            }
        }),
    }
}

fn sorted_unique_times(times: impl Iterator<Item = f64>) -> Vec<f64> {
    let mut values = times.collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    values.dedup_by(|left, right| left.total_cmp(right).is_eq());
    values
}

fn kaplan_meier_steps(records: &[SurvivalRecord], groups: &[String]) -> Vec<SurvivalKmStep> {
    let mut steps = Vec::new();
    for group in groups {
        let group_records = records
            .iter()
            .filter(|record| &record.group == group)
            .collect::<Vec<_>>();
        let times = sorted_unique_times(group_records.iter().map(|record| record.time));
        let mut survival = 1.0;
        let mut greenwood = 0.0;
        for time in times {
            let n_risk = group_records
                .iter()
                .filter(|record| record.time >= time)
                .count();
            let n_event = group_records
                .iter()
                .filter(|record| record.time == time && record.event)
                .count();
            let n_censored = group_records
                .iter()
                .filter(|record| record.time == time && !record.event)
                .count();
            if n_event == 0 {
                continue;
            }
            let risk = n_risk as f64;
            let event = n_event as f64;
            survival *= 1.0 - event / risk;
            if n_risk > n_event {
                greenwood += event / (risk * (risk - event));
            }
            let standard_error = survival * greenwood.sqrt();
            let (ci_lower, ci_upper) = normal_ci(survival, standard_error);
            steps.push(SurvivalKmStep {
                group: group.clone(),
                time,
                n_risk,
                n_event,
                n_censored,
                survival,
                standard_error,
                ci_lower,
                ci_upper,
            });
        }
    }
    steps
}

fn normal_ci(estimate: f64, standard_error: f64) -> (f64, f64) {
    if !standard_error.is_finite() {
        return (f64::NAN, f64::NAN);
    }
    (
        (estimate - 1.96 * standard_error).clamp(0.0, 1.0),
        (estimate + 1.96 * standard_error).clamp(0.0, 1.0),
    )
}

fn log_rank_test(records: &[SurvivalRecord], groups: &[String]) -> LogRankResult {
    let event_times = sorted_unique_times(
        records
            .iter()
            .filter(|record| record.event)
            .map(|record| record.time),
    );
    let mut observed = vec![0.0; groups.len()];
    let mut expected = vec![0.0; groups.len()];
    let mut variance = vec![0.0; groups.len()];

    for time in event_times {
        let total_risk = records.iter().filter(|record| record.time >= time).count() as f64;
        let total_events = records
            .iter()
            .filter(|record| record.time == time && record.event)
            .count() as f64;
        if total_risk <= 1.0 || total_events == 0.0 {
            continue;
        }
        for (group_index, group) in groups.iter().enumerate() {
            let group_risk = records
                .iter()
                .filter(|record| record.time >= time && &record.group == group)
                .count() as f64;
            let group_events = records
                .iter()
                .filter(|record| record.time == time && record.event && &record.group == group)
                .count() as f64;
            observed[group_index] += group_events;
            expected[group_index] += group_risk * total_events / total_risk;
            variance[group_index] +=
                group_risk * (total_risk - group_risk) * total_events * (total_risk - total_events)
                    / (total_risk * total_risk * (total_risk - 1.0));
        }
    }

    let chi_square = observed
        .iter()
        .zip(expected.iter())
        .zip(variance.iter())
        .filter(|(_, var)| **var > 0.0)
        .map(|((obs, exp), var)| {
            let diff = obs - exp;
            diff * diff / var
        })
        .sum::<f64>()
        / 2.0;
    let degrees_freedom = groups.len().saturating_sub(1);
    let p_value = 1.0 - chi_square_cdf(chi_square, degrees_freedom as f64);

    LogRankResult {
        chi_square,
        degrees_freedom,
        p_value: p_value.clamp(0.0, 1.0),
        groups: groups.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn temp_csv(name: &str, content: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "stats-code-survival-{name}-{}.csv",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::write(&path, content).expect("write csv");
        path
    }

    #[test]
    fn kaplan_meier_overall_matches_hand_calculation() {
        let path = temp_csv("overall", "time,event\n1,1\n2,0\n3,1\n4,1\n");
        let args = SurvivalKmArgs {
            data: Some(path.clone()),
            analysis: None,
            time: "time".to_string(),
            event: "event".to_string(),
            group: None,
        };

        let result = survival_km_csv(&path, None, &args).expect("km");

        assert_eq!(result.n_used, 4);
        assert_eq!(result.steps.len(), 3);
        assert!((result.steps[0].survival - 0.75).abs() < 1e-12);
        assert!((result.steps[1].survival - 0.375).abs() < 1e-12);
        assert!((result.steps[2].survival - 0.0).abs() < 1e-12);
        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn log_rank_is_reported_for_grouped_survival() {
        let path = temp_csv(
            "grouped",
            "time,event,arm\n1,1,A\n2,1,A\n3,0,A\n2,0,B\n4,1,B\n5,1,B\n",
        );
        let args = SurvivalKmArgs {
            data: Some(path.clone()),
            analysis: None,
            time: "time".to_string(),
            event: "event".to_string(),
            group: Some("arm".to_string()),
        };

        let result = survival_km_csv(&path, None, &args).expect("km");

        assert_eq!(result.groups, vec!["A".to_string(), "B".to_string()]);
        assert!(result.log_rank.is_some());
        let log_rank = result.log_rank.expect("log-rank");
        assert_eq!(log_rank.degrees_freedom, 1);
        assert!(log_rank.p_value.is_finite());
        fs::remove_file(path).expect("cleanup");
    }
}
