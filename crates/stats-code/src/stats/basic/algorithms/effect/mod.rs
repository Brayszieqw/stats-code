use std::collections::BTreeMap;

use crate::cli::NaStrategy;
use crate::helpers::require_column;
use crate::schema::{MhStratum, OrRrResult, TwoByTwoCells};

use super::common::{
    check_missing_policy, chi_square_p_value, column_index, event_value, missing, prelude_notes,
    z_critical, EPS,
};

pub(crate) fn or_rr_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    exposure_col: &str,
    outcome_col: &str,
    strata_cols: &[String],
    exposure_event: Option<&str>,
    outcome_event: Option<&str>,
    alpha: f64,
    strategy: NaStrategy,
) -> Result<OrRrResult, String> {
    let index = column_index(headers);
    let ie = require_column(&index, exposure_col)?;
    let io = require_column(&index, outcome_col)?;
    let strata_idx = strata_cols
        .iter()
        .map(|s| require_column(&index, s).map(|idx| (s.clone(), idx)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut by_stratum: BTreeMap<String, [usize; 4]> = BTreeMap::new();
    let mut excluded = 0usize;
    for row in rows {
        let re = row.get(ie).unwrap_or("").trim();
        let ro = row.get(io).unwrap_or("").trim();
        let Some(exposed) = event_value(re, exposure_col, exposure_event) else {
            excluded += 1;
            continue;
        };
        let Some(event) = event_value(ro, outcome_col, outcome_event) else {
            excluded += 1;
            continue;
        };
        let mut label_parts = Vec::new();
        let mut missing_stratum = false;
        for (name, idx) in &strata_idx {
            let raw = row.get(*idx).unwrap_or("").trim();
            if missing(name, raw) {
                missing_stratum = true;
                break;
            }
            label_parts.push(format!("{name}={raw}"));
        }
        if missing_stratum {
            excluded += 1;
            continue;
        }
        let label = if label_parts.is_empty() {
            "__crude__".to_string()
        } else {
            label_parts.join("|")
        };
        let cells = by_stratum.entry(label).or_insert([0, 0, 0, 0]);
        match (exposed, event) {
            (true, true) => cells[0] += 1,
            (true, false) => cells[1] += 1,
            (false, true) => cells[2] += 1,
            (false, false) => cells[3] += 1,
        }
    }
    check_missing_policy(excluded, strategy, "OR/RR")?;
    let crude_counts = by_stratum.values().fold([0usize; 4], |mut acc, c| {
        for i in 0..4 {
            acc[i] += c[i];
        }
        acc
    });
    let (cells, corrected) = corrected_cells(crude_counts);
    let z = z_critical(alpha);
    let odds_ratio = cells.a * cells.d / (cells.b * cells.c).max(EPS);
    let se_or = (1.0 / cells.a + 1.0 / cells.b + 1.0 / cells.c + 1.0 / cells.d).sqrt();
    let or_ci_lower = (odds_ratio.ln() - z * se_or).exp();
    let or_ci_upper = (odds_ratio.ln() + z * se_or).exp();
    let risk_e = cells.a / (cells.a + cells.b).max(EPS);
    let risk_u = cells.c / (cells.c + cells.d).max(EPS);
    let relative_risk = risk_e / risk_u.max(EPS);
    let se_rr = (cells.b / (cells.a * (cells.a + cells.b)).max(EPS)
        + cells.d / (cells.c * (cells.c + cells.d)).max(EPS))
    .sqrt();
    let rr_ci_lower = (relative_risk.ln() - z * se_rr).exp();
    let rr_ci_upper = (relative_risk.ln() + z * se_rr).exp();
    let n = cells.a + cells.b + cells.c + cells.d;
    let chi_square = n * (cells.a * cells.d - cells.b * cells.c).powi(2)
        / ((cells.a + cells.b) * (cells.c + cells.d) * (cells.a + cells.c) * (cells.b + cells.d))
            .max(EPS);
    let mut warnings = Vec::new();
    if corrected {
        warnings.push(
            "0.5 continuity correction applied because at least one 2x2 cell is zero.".to_string(),
        );
    }
    let mut mh_strata = Vec::new();
    let mut mh_cells = Vec::new();
    let mut any_stratum_corrected = false;
    for (label, raw_cells) in &by_stratum {
        let (scells, stratum_corrected) = corrected_cells(*raw_cells);
        any_stratum_corrected |= stratum_corrected;
        mh_strata.push(MhStratum {
            label: label.clone(),
            cells: scells.clone(),
            or_stratum: odds_ratio_for_cells(&scells),
            rr_stratum: risk_ratio_for_cells(&scells),
        });
        mh_cells.push(scells);
    }
    if any_stratum_corrected && !corrected {
        warnings.push(
            "0.5 continuity correction applied within at least one stratum because a 2x2 cell is zero."
                .to_string(),
        );
    }
    let mh_or = if strata_cols.is_empty() {
        None
    } else {
        mantel_haenszel_or(&mh_cells)
    };
    let mh_or_se = if strata_cols.is_empty() {
        None
    } else {
        mantel_haenszel_log_or_se(&mh_cells)
    };
    let mh_or_ci_lower = mh_or.zip(mh_or_se).map(|(or, se)| (or.ln() - z * se).exp());
    let mh_or_ci_upper = mh_or.zip(mh_or_se).map(|(or, se)| (or.ln() + z * se).exp());
    let mh_rr_with_se = if strata_cols.is_empty() {
        None
    } else {
        mantel_haenszel_rr_and_se(&mh_cells)
    };
    let mh_rr = mh_rr_with_se.map(|(rr, _)| rr);
    let mh_rr_ci_lower = mh_rr_with_se.map(|(rr, se)| (rr.ln() - z * se).exp());
    let mh_rr_ci_upper = mh_rr_with_se.map(|(rr, se)| (rr.ln() + z * se).exp());
    let (homogeneity_chi_square, homogeneity_p) = if strata_cols.is_empty() {
        (None, None)
    } else {
        mh_or
            .and_then(|or| breslow_day_test(&mh_cells, or))
            .map_or((None, None), |(stat, p)| (Some(stat), Some(p)))
    };
    Ok(OrRrResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: crude_counts.iter().sum(),
        n_excluded_missing: excluded,
        notes: prelude_notes(crude_counts.iter().sum(), rows.len(), excluded),
        warnings,
        exposure: exposure_col.to_string(),
        outcome: outcome_col.to_string(),
        cells,
        odds_ratio,
        or_ci_lower,
        or_ci_upper,
        relative_risk,
        rr_ci_lower,
        rr_ci_upper,
        chi_square,
        chi_p_value: chi_square_p_value(chi_square, 1.0),
        continuity_correction: corrected || any_stratum_corrected,
        mh_or,
        mh_or_ci_lower,
        mh_or_ci_upper,
        mh_rr,
        mh_rr_ci_lower,
        mh_rr_ci_upper,
        mh_strata,
        homogeneity_chi_square,
        homogeneity_p,
    })
}

fn corrected_cells(raw: [usize; 4]) -> (TwoByTwoCells, bool) {
    let corrected = raw.contains(&0);
    let add = if corrected { 0.5 } else { 0.0 };
    (
        TwoByTwoCells {
            a: raw[0] as f64 + add,
            b: raw[1] as f64 + add,
            c: raw[2] as f64 + add,
            d: raw[3] as f64 + add,
        },
        corrected,
    )
}

fn odds_ratio_for_cells(cells: &TwoByTwoCells) -> f64 {
    cells.a * cells.d / (cells.b * cells.c).max(EPS)
}

fn risk_ratio_for_cells(cells: &TwoByTwoCells) -> f64 {
    let exposed_risk = cells.a / (cells.a + cells.b).max(EPS);
    let unexposed_risk = cells.c / (cells.c + cells.d).max(EPS);
    exposed_risk / unexposed_risk.max(EPS)
}

fn mantel_haenszel_or(strata: &[TwoByTwoCells]) -> Option<f64> {
    let mut sum_ad_over_n = 0.0;
    let mut sum_bc_over_n = 0.0;
    for cells in strata {
        let n = cells.a + cells.b + cells.c + cells.d;
        if n <= EPS {
            continue;
        }
        sum_ad_over_n += cells.a * cells.d / n;
        sum_bc_over_n += cells.b * cells.c / n;
    }
    if sum_ad_over_n > 0.0 && sum_bc_over_n > 0.0 {
        Some(sum_ad_over_n / sum_bc_over_n)
    } else {
        None
    }
}

fn mantel_haenszel_log_or_se(strata: &[TwoByTwoCells]) -> Option<f64> {
    let mut r = 0.0;
    let mut s = 0.0;
    let mut term1 = 0.0;
    let mut term2 = 0.0;
    let mut term3 = 0.0;
    for cells in strata {
        let n = cells.a + cells.b + cells.c + cells.d;
        if n <= EPS {
            continue;
        }
        let ri = cells.a * cells.d / n;
        let si = cells.b * cells.c / n;
        let p = (cells.a + cells.d) / n;
        let q = (cells.b + cells.c) / n;
        r += ri;
        s += si;
        term1 += p * ri;
        term2 += p * si + q * ri;
        term3 += q * si;
    }
    if r <= EPS || s <= EPS {
        return None;
    }
    let variance = 0.5 * (term1 / r.powi(2) + term2 / (r * s) + term3 / s.powi(2));
    variance.is_finite().then_some(variance.max(0.0).sqrt())
}

fn mantel_haenszel_rr_and_se(strata: &[TwoByTwoCells]) -> Option<(f64, f64)> {
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    let mut var_num = 0.0;
    let mut var_den = 0.0;
    for cells in strata {
        let exposed_n = cells.a + cells.b;
        let unexposed_n = cells.c + cells.d;
        let n = exposed_n + unexposed_n;
        if exposed_n <= EPS || unexposed_n <= EPS || n <= EPS {
            continue;
        }
        numerator += cells.a * unexposed_n / n;
        denominator += cells.c * exposed_n / n;
        var_num += (unexposed_n / n).powi(2) * cells.a * cells.b / exposed_n;
        var_den += (exposed_n / n).powi(2) * cells.c * cells.d / unexposed_n;
    }
    if numerator <= EPS || denominator <= EPS {
        return None;
    }
    let rr = numerator / denominator;
    let variance = var_num / numerator.powi(2) + var_den / denominator.powi(2);
    if rr.is_finite() && variance.is_finite() {
        Some((rr, variance.max(0.0).sqrt()))
    } else {
        None
    }
}

fn breslow_day_test(strata: &[TwoByTwoCells], common_or: f64) -> Option<(f64, f64)> {
    if strata.len() < 2 || !common_or.is_finite() || common_or <= 0.0 {
        return None;
    }
    let mut statistic = 0.0;
    let mut usable = 0usize;
    for cells in strata {
        let expected_a = expected_a_under_common_or(cells, common_or)?;
        let exposed_n = cells.a + cells.b;
        let unexposed_n = cells.c + cells.d;
        let events_n = cells.a + cells.c;
        let expected_b = exposed_n - expected_a;
        let expected_c = events_n - expected_a;
        let expected_d = unexposed_n - expected_c;
        let variance_inv = 1.0 / expected_a.max(EPS)
            + 1.0 / expected_b.max(EPS)
            + 1.0 / expected_c.max(EPS)
            + 1.0 / expected_d.max(EPS);
        let variance = 1.0 / variance_inv.max(EPS);
        if variance > EPS && variance.is_finite() {
            statistic += (cells.a - expected_a).powi(2) / variance;
            usable += 1;
        }
    }
    if usable < 2 {
        return None;
    }
    let df = usable as f64 - 1.0;
    Some((statistic, chi_square_p_value(statistic, df)))
}

fn expected_a_under_common_or(cells: &TwoByTwoCells, common_or: f64) -> Option<f64> {
    let exposed_n = cells.a + cells.b;
    let unexposed_n = cells.c + cells.d;
    let events_n = cells.a + cells.c;
    let non_events_n = cells.b + cells.d;
    let n = exposed_n + unexposed_n;
    if n <= EPS {
        return None;
    }
    if (common_or - 1.0).abs() < 1e-10 {
        return Some(exposed_n * events_n / n);
    }
    let qa = 1.0 - common_or;
    let qb = non_events_n - exposed_n + common_or * (exposed_n + events_n);
    let qc = -common_or * exposed_n * events_n;
    let disc = (qb * qb - 4.0 * qa * qc).max(0.0);
    let sqrt_disc = disc.sqrt();
    let lower = (events_n - unexposed_n).max(0.0);
    let upper = exposed_n.min(events_n);
    let roots = [
        (-qb + sqrt_disc) / (2.0 * qa),
        (-qb - sqrt_disc) / (2.0 * qa),
    ];
    roots
        .iter()
        .copied()
        .find(|root| *root >= lower - 1e-8 && *root <= upper + 1e-8)
        .or_else(|| {
            roots
                .iter()
                .copied()
                .filter(|root| root.is_finite())
                .min_by(|a, b| {
                    let da = if *a < lower {
                        lower - *a
                    } else if *a > upper {
                        *a - upper
                    } else {
                        0.0
                    };
                    let db = if *b < lower {
                        lower - *b
                    } else if *b > upper {
                        *b - upper
                    } else {
                        0.0
                    };
                    da.total_cmp(&db)
                })
                .map(|root| root.clamp(lower, upper))
        })
}
