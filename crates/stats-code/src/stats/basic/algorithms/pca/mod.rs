use crate::cli::NaStrategy;
use crate::helpers::require_column;
use crate::math::{chi_square_cdf, invert_matrix, jacobi_eigh, matrix_determinant};
use crate::schema::{PcaComponent, PcaResult};

use super::common::{check_missing_policy, column_index, missing, parse_num, prelude_notes, EPS};

pub(crate) fn pca_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    vars: &[String],
    n_components: Option<usize>,
    matrix_kind: &str,
    strategy: NaStrategy,
) -> Result<PcaResult, String> {
    let data = numeric_matrix(rows, headers, vars, strategy)?;
    let (matrix, kept_vars, excluded_variables) =
        covariance_or_correlation(&data.values, vars, matrix_kind);
    if matrix.is_empty() {
        return Err("PCA requires at least one non-constant variable.".to_string());
    }
    let (eigenvalues, eigenvectors) = jacobi_eigh(matrix.clone());
    let total = eigenvalues.iter().sum::<f64>().max(EPS);
    let keep = n_components
        .unwrap_or(eigenvalues.len())
        .min(eigenvalues.len());
    let mut components = Vec::new();
    let mut cumulative = 0.0;
    for (i, eigenvalue) in eigenvalues.iter().take(keep).enumerate() {
        let prop = *eigenvalue / total;
        cumulative += prop;
        components.push(PcaComponent {
            component: i + 1,
            eigenvalue: *eigenvalue,
            variance_explained: prop,
            cumulative_variance: cumulative,
        });
    }
    let loadings = eigenvectors
        .iter()
        .map(|row| row.iter().take(keep).copied().collect())
        .collect();

    // KMO and Bartlett's test of sphericity
    let (kmo, bartlett_chi_square, bartlett_df, bartlett_p) =
        compute_kmo_and_bartlett(&matrix, data.n_used);

    Ok(PcaResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: data.n_used,
        n_excluded_missing: data.n_excluded,
        notes: prelude_notes(data.n_used, rows.len(), data.n_excluded),
        warnings: if excluded_variables.is_empty() {
            vec![]
        } else {
            vec![format!(
                "Excluded zero-variance variables: {}",
                excluded_variables.join(", ")
            )]
        },
        variables: kept_vars,
        components,
        loadings,
        kmo,
        bartlett_chi_square,
        bartlett_df,
        bartlett_p,
        excluded_variables,
    })
}

pub(super) struct NumericMatrix {
    pub(super) values: Vec<Vec<f64>>,
    pub(super) n_used: usize,
    pub(super) n_excluded: usize,
}

pub(super) fn numeric_matrix(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    vars: &[String],
    strategy: NaStrategy,
) -> Result<NumericMatrix, String> {
    let index = column_index(headers);
    let indices = vars
        .iter()
        .map(|v| require_column(&index, v).map(|idx| (v.clone(), idx)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut values = Vec::new();
    let mut excluded = 0usize;
    for row in rows {
        let mut out = Vec::new();
        let mut bad = false;
        for (name, idx) in &indices {
            let raw = row.get(*idx).unwrap_or("").trim();
            if missing(name, raw) {
                bad = true;
                break;
            }
            out.push(parse_num(raw, name)?);
        }
        if bad {
            excluded += 1;
        } else {
            values.push(out);
        }
    }
    check_missing_policy(excluded, strategy, "numeric matrix")?;
    Ok(NumericMatrix {
        n_used: values.len(),
        values,
        n_excluded: excluded,
    })
}

fn covariance_or_correlation(
    data: &[Vec<f64>],
    vars: &[String],
    kind: &str,
) -> (Vec<Vec<f64>>, Vec<String>, Vec<String>) {
    let p = data.first().map_or(0, Vec::len);
    let n = data.len();
    let mut means = vec![0.0; p];
    for row in data {
        for (j, value) in row.iter().enumerate() {
            means[j] += value;
        }
    }
    for m in &mut means {
        *m /= n.max(1) as f64;
    }
    let mut vars_sample = vec![0.0; p];
    for row in data {
        for j in 0..p {
            vars_sample[j] += (row[j] - means[j]).powi(2);
        }
    }
    for v in &mut vars_sample {
        *v /= (n.saturating_sub(1)).max(1) as f64;
    }
    let kept_indices: Vec<usize> = vars_sample
        .iter()
        .enumerate()
        .filter_map(|(i, v)| if *v > EPS { Some(i) } else { None })
        .collect();
    let excluded: Vec<String> = vars
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            if vars_sample[i] <= EPS {
                Some(v.clone())
            } else {
                None
            }
        })
        .collect();
    let kept_vars: Vec<String> = kept_indices.iter().map(|i| vars[*i].clone()).collect();
    let q = kept_indices.len();
    let mut matrix = vec![vec![0.0; q]; q];
    for (a_pos, &a) in kept_indices.iter().enumerate() {
        for (b_pos, &b) in kept_indices.iter().enumerate() {
            let cov = data
                .iter()
                .map(|row| (row[a] - means[a]) * (row[b] - means[b]))
                .sum::<f64>()
                / (n.saturating_sub(1)).max(1) as f64;
            matrix[a_pos][b_pos] = if kind.eq_ignore_ascii_case("covariance") {
                cov
            } else {
                cov / (vars_sample[a].sqrt() * vars_sample[b].sqrt()).max(EPS)
            };
        }
    }
    (matrix, kept_vars, excluded)
}

/// Compute KMO and Bartlett's test of sphericity for a correlation matrix.
fn compute_kmo_and_bartlett(matrix: &[Vec<f64>], n: usize) -> (f64, f64, usize, f64) {
    let p = matrix.len();
    if p < 2 || n < 3 {
        return (f64::NAN, f64::NAN, 0, f64::NAN);
    }

    // KMO via anti-image correlation matrix
    let kmo = {
        let precision = match invert_matrix(matrix) {
            Ok(pm) => pm,
            Err(_) => return (f64::NAN, f64::NAN, 0, f64::NAN),
        };
        // Anti-image correlation: a_ij = -p_ij / sqrt(p_ii * p_jj) for i != j
        let mut sum_r_sq = 0.0;
        let mut sum_a_sq = 0.0;
        for i in 0..p {
            for j in 0..p {
                if i == j {
                    continue;
                }
                sum_r_sq += matrix[i][j] * matrix[i][j];
                let a_ij = -precision[i][j] / (precision[i][i] * precision[j][j]).sqrt().max(EPS);
                sum_a_sq += a_ij * a_ij;
            }
        }
        let denom = sum_r_sq + sum_a_sq;
        if denom > EPS {
            sum_r_sq / denom
        } else {
            f64::NAN
        }
    };

    // Bartlett's test: chi^2 = -(n - 1 - (2p + 5)/6) * ln(|R|)
    let bartlett = {
        let det = matrix_determinant(matrix);
        if det <= EPS {
            (f64::INFINITY, (p * (p - 1) / 2), 0.0)
        } else {
            let n_f = n as f64;
            let correction = n_f - 1.0 - (2.0 * p as f64 + 5.0) / 6.0;
            let chi_sq = -correction * det.ln();
            let df = p * (p - 1) / 2;
            let p_val = (1.0 - chi_square_cdf(chi_sq, df as f64)).clamp(0.0, 1.0);
            (chi_sq, df, p_val)
        }
    };

    (kmo, bartlett.0, bartlett.1, bartlett.2)
}
