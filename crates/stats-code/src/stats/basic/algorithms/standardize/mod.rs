use std::collections::BTreeMap;
use std::path::Path;

use crate::cli::NaStrategy;
use crate::helpers::require_column;
use crate::schema::{StandardizationResult, StandardizationStratum};

use super::common::{
    check_missing_policy, column_index, missing, parse_num, prelude_notes, stringify_csv_error,
    z_critical, EPS,
};

pub(crate) fn standardize_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    method: &str,
    event_col: &str,
    person_time_col: &str,
    age_group_col: &str,
    standard_pop: &str,
    alpha: f64,
    strategy: NaStrategy,
) -> Result<StandardizationResult, String> {
    let index = column_index(headers);
    let ie = require_column(&index, event_col)?;
    let ipt = require_column(&index, person_time_col)?;
    let ia = require_column(&index, age_group_col)?;
    let mut agg: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    let mut excluded = 0usize;
    let mut warnings = Vec::new();
    for row in rows {
        let re = row.get(ie).unwrap_or("").trim();
        let rpt = row.get(ipt).unwrap_or("").trim();
        let rage = row.get(ia).unwrap_or("").trim();
        if missing(event_col, re) || missing(person_time_col, rpt) || missing(age_group_col, rage) {
            excluded += 1;
            continue;
        }
        let events = parse_num(re, event_col)?;
        let pt = parse_num(rpt, person_time_col)?;
        if pt <= 0.0 {
            warnings.push(format!(
                "stratum `{rage}` excluded because person-time is zero or negative"
            ));
            excluded += 1;
            continue;
        }
        let entry = agg.entry(rage.to_string()).or_default();
        entry.0 += events;
        entry.1 += pt;
    }
    check_missing_policy(excluded, strategy, "standardization")?;
    if agg.is_empty() {
        return Err("Standardization requires at least one non-empty stratum.".to_string());
    }
    let (weights, weight_warnings) = standardization_weights(standard_pop, &agg)?;
    warnings.extend(weight_warnings);
    let z = z_critical(alpha);
    let mut strata = Vec::new();
    let mut std_rate = 0.0;
    let mut var_direct = 0.0;
    let mut observed = 0.0;
    let mut expected = 0.0;
    for (age, (events, pt)) in &agg {
        let weight = *weights.get(age).unwrap_or(&0.0);
        let rate = events / pt;
        std_rate += weight * rate;
        var_direct += weight.powi(2) * events.max(1.0) / pt.powi(2);
        observed += events;
        strata.push(StandardizationStratum {
            age_group: age.clone(),
            observed: *events,
            expected: pt * std_rate.max(EPS),
            weight,
            stratum_rate: rate,
        });
    }
    for stratum in &mut strata {
        let pt = agg
            .get(&stratum.age_group)
            .map_or(0.0, |(_, person_time)| *person_time);
        stratum.expected = pt * std_rate.max(EPS);
        expected += stratum.expected;
    }
    if method.eq_ignore_ascii_case("indirect") {
        let smr = observed / expected.max(EPS);
        let se = 1.0 / observed.max(1.0).sqrt();
        Ok(StandardizationResult {
            status: "ok".to_string(),
            data_path: String::new(),
            analysis_path: None,
            n_total: rows.len(),
            n_used: rows.len() - excluded,
            n_excluded_missing: excluded,
            notes: prelude_notes(rows.len() - excluded, rows.len(), excluded),
            warnings,
            method: "indirect".to_string(),
            strata,
            standardized_rate: None,
            direct_ci_lower: None,
            direct_ci_upper: None,
            smr: Some(smr),
            smr_ci_lower: Some((smr.ln() - z * se).exp()),
            smr_ci_upper: Some((smr.ln() + z * se).exp()),
        })
    } else {
        let se = var_direct.sqrt();
        Ok(StandardizationResult {
            status: "ok".to_string(),
            data_path: String::new(),
            analysis_path: None,
            n_total: rows.len(),
            n_used: rows.len() - excluded,
            n_excluded_missing: excluded,
            notes: prelude_notes(rows.len() - excluded, rows.len(), excluded),
            warnings,
            method: "direct".to_string(),
            strata,
            standardized_rate: Some(std_rate),
            direct_ci_lower: Some((std_rate - z * se).max(0.0)),
            direct_ci_upper: Some(std_rate + z * se),
            smr: None,
            smr_ci_lower: None,
            smr_ci_upper: None,
        })
    }
}

fn standardization_weights(
    standard_pop: &str,
    agg: &BTreeMap<String, (f64, f64)>,
) -> Result<(BTreeMap<String, f64>, Vec<String>), String> {
    let raw_weights = if let Some(builtin) = builtin_standard_population(standard_pop) {
        builtin
    } else if Path::new(standard_pop).is_file() {
        read_standard_population_csv(Path::new(standard_pop))?
    } else {
        BTreeMap::new()
    };
    let mut warnings = Vec::new();
    let mut weights = BTreeMap::new();
    if raw_weights.is_empty() {
        let equal = 1.0 / agg.len() as f64;
        for age in agg.keys() {
            weights.insert(age.clone(), equal);
        }
        warnings.push(format!(
            "standard population `{standard_pop}` was not recognized; using equal weights across observed strata"
        ));
        return Ok((weights, warnings));
    }
    let matched_total = agg
        .keys()
        .filter_map(|age| raw_weights.get(age))
        .sum::<f64>();
    if matched_total <= 0.0 {
        let equal = 1.0 / agg.len() as f64;
        for age in agg.keys() {
            weights.insert(age.clone(), equal);
        }
        warnings.push(format!(
            "standard population `{standard_pop}` had no matching age strata; using equal weights"
        ));
        return Ok((weights, warnings));
    }
    for age in agg.keys() {
        let weight = raw_weights.get(age).copied().unwrap_or(0.0) / matched_total;
        if weight == 0.0 {
            warnings.push(format!(
                "standard population has no weight for observed stratum `{age}`"
            ));
        }
        weights.insert(age.clone(), weight);
    }
    Ok((weights, warnings))
}

fn builtin_standard_population(name: &str) -> Option<BTreeMap<String, f64>> {
    let values: &[(&str, f64)] = match name.to_ascii_lowercase().as_str() {
        "who_world_2000" => &[
            ("0-4", 8.86),
            ("5-9", 8.69),
            ("10-14", 8.60),
            ("15-19", 8.47),
            ("20-24", 8.22),
            ("25-29", 7.93),
            ("30-34", 7.61),
            ("35-39", 7.15),
            ("40-44", 6.59),
            ("45-49", 6.04),
            ("50-54", 5.37),
            ("55-59", 4.55),
            ("60-64", 3.72),
            ("65-69", 2.96),
            ("70-74", 2.21),
            ("75-79", 1.52),
            ("80-84", 0.91),
            ("85+", 0.63),
        ],
        "segi_world" => &[
            ("0-4", 12.0),
            ("5-9", 10.0),
            ("10-14", 9.0),
            ("15-19", 9.0),
            ("20-24", 8.0),
            ("25-29", 8.0),
            ("30-34", 6.0),
            ("35-39", 6.0),
            ("40-44", 6.0),
            ("45-49", 6.0),
            ("50-54", 5.0),
            ("55-59", 4.0),
            ("60-64", 4.0),
            ("65-69", 3.0),
            ("70-74", 2.0),
            ("75-79", 1.0),
            ("80-84", 0.5),
            ("85+", 0.5),
        ],
        "china_census_2010" => &[
            ("0-4", 6.0),
            ("5-9", 5.4),
            ("10-14", 5.4),
            ("15-19", 7.0),
            ("20-24", 9.0),
            ("25-29", 8.0),
            ("30-34", 7.6),
            ("35-39", 8.2),
            ("40-44", 9.0),
            ("45-49", 8.4),
            ("50-54", 7.2),
            ("55-59", 6.4),
            ("60-64", 4.9),
            ("65-69", 3.5),
            ("70-74", 2.5),
            ("75-79", 1.7),
            ("80-84", 1.0),
            ("85+", 0.8),
        ],
        _ => return None,
    };
    Some(
        values
            .iter()
            .map(|(age, weight)| ((*age).to_string(), *weight))
            .collect(),
    )
}

fn read_standard_population_csv(path: &Path) -> Result<BTreeMap<String, f64>, String> {
    let mut reader = csv::Reader::from_path(path).map_err(|error| {
        format!(
            "Cannot read standard population `{}`: {error}",
            path.display()
        )
    })?;
    let headers = reader.headers().map_err(stringify_csv_error)?.clone();
    let index = column_index(&headers);
    let age_idx = index.get("age_group").copied().unwrap_or(0);
    let weight_idx = index
        .get("weight")
        .or_else(|| index.get("population"))
        .or_else(|| index.get("standard_population"))
        .copied()
        .unwrap_or(1);
    let mut weights = BTreeMap::new();
    for record in reader.records() {
        let record = record.map_err(stringify_csv_error)?;
        let age = record.get(age_idx).unwrap_or("").trim();
        let weight_raw = record.get(weight_idx).unwrap_or("").trim();
        if age.is_empty() || weight_raw.is_empty() {
            continue;
        }
        let weight = weight_raw
            .parse::<f64>()
            .map_err(|_| format!("Standard population weight `{weight_raw}` is not numeric."))?;
        if weight > 0.0 {
            weights.insert(age.to_string(), weight);
        }
    }
    Ok(weights)
}
