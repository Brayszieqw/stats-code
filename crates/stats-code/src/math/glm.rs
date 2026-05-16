// ---------------------------------------------------------------------------
// Generic IRLS (Iteratively Reweighted Least Squares) engine.
// Implements task 2.2 — shared by Poisson GLM (Req 13) and dose-response (Req 20).
// ---------------------------------------------------------------------------

//! Generic IRLS engine for generalized linear models.
//!
//! Currently a stub — full implementation is task 2.2.

use super::invert_matrix_with_ridge;

/// Result of an IRLS fit.
#[derive(Debug, Clone)]
pub(crate) struct IrlsFit {
    /// Estimated coefficients.
    pub beta: Vec<f64>,
    /// Variance-covariance matrix (p × p).
    pub vcov: Vec<Vec<f64>>,
    /// Number of iterations performed.
    pub iterations: usize,
    /// Whether the algorithm converged.
    pub converged: bool,
    /// Residual deviance.
    pub deviance: f64,
    /// Pearson chi-square statistic.
    pub pearson_chi_square: f64,
    /// Log-likelihood at convergence.
    pub log_likelihood: f64,
}

/// GLM family trait — defines the link function and variance function.
pub(crate) trait Family {
    /// Mean from linear predictor (inverse link).
    fn mean(eta: f64) -> f64;
    /// Derivative of mean w.r.t. eta (d mu / d eta).
    fn d_mean(eta: f64) -> f64;
    /// Variance as a function of the mean.
    fn variance(mu: f64) -> f64;
    /// Deviance contribution for one observation.
    fn deviance_contribution(y: f64, mu: f64) -> f64;
    /// Log-likelihood contribution for one observation.
    fn log_likelihood_contribution(y: f64, mu: f64) -> f64;
}

/// Poisson family with log link.
pub(crate) struct Poisson;

impl Family for Poisson {
    fn mean(eta: f64) -> f64 {
        eta.exp().clamp(1e-300, f64::MAX)
    }

    fn d_mean(eta: f64) -> f64 {
        eta.exp().clamp(1e-300, f64::MAX)
    }

    fn variance(mu: f64) -> f64 {
        mu.max(1e-300)
    }

    fn deviance_contribution(y: f64, mu: f64) -> f64 {
        if y <= 0.0 {
            2.0 * mu
        } else {
            2.0 * (y * (y / mu).ln() - (y - mu))
        }
    }

    fn log_likelihood_contribution(y: f64, mu: f64) -> f64 {
        // Poisson log-likelihood: y*log(mu) - mu - log(y!)
        // We omit the log(y!) constant since it cancels in comparisons.
        y * mu.ln() - mu
    }
}

/// Fit a GLM via IRLS.
///
/// # Arguments
/// * `x` — design matrix (n × p), including intercept column if desired
/// * `y` — response vector (n)
/// * `offset` — optional offset vector (n); pass `None` or all-zeros for no offset
/// * `max_iter` — maximum number of IRLS iterations (default 25)
/// * `tol` — convergence tolerance on relative change in beta (default 1e-6)
///
/// # Type parameter
/// `F` must implement [`Family`] (e.g., [`Poisson`]).
pub(crate) fn irls_fit<F: Family>(
    x: &[Vec<f64>],
    y: &[f64],
    offset: Option<&[f64]>,
    max_iter: usize,
    tol: f64,
) -> Result<IrlsFit, String> {
    let n = y.len();
    let p = if x.is_empty() { 0 } else { x[0].len() };
    if n == 0 || p == 0 {
        return Err("Empty design matrix or response vector.".to_string());
    }
    if x.len() != n {
        return Err(format!(
            "Design matrix has {} rows but response has {} elements.",
            x.len(),
            n
        ));
    }

    let zero_offset = vec![0.0_f64; n];
    let off = offset.unwrap_or(&zero_offset);

    // Initialize beta to zeros.
    let mut beta = vec![0.0_f64; p];

    let mut converged = false;
    let mut iterations = 0usize;

    for _iter in 0..max_iter {
        iterations += 1;

        // Compute linear predictor η = X β + offset
        let eta: Vec<f64> = x
            .iter()
            .zip(off.iter())
            .map(|(row, &o)| {
                let xb: f64 = row.iter().zip(beta.iter()).map(|(xi, bi)| xi * bi).sum();
                xb + o
            })
            .collect();

        // Compute μ = g⁻¹(η), W = diag(μ / V(μ) * (dμ/dη)²), z = η + (y-μ)/(dμ/dη)
        let mu: Vec<f64> = eta.iter().map(|&e| F::mean(e)).collect();
        let w: Vec<f64> = eta
            .iter()
            .zip(mu.iter())
            .map(|(&e, &m)| {
                let dm = F::d_mean(e);
                let v = F::variance(m);
                if v <= 0.0 || dm == 0.0 {
                    0.0
                } else {
                    dm * dm / v
                }
            })
            .collect();
        let z: Vec<f64> = eta
            .iter()
            .zip(y.iter())
            .zip(mu.iter())
            .zip(eta.iter())
            .map(|(((&e, &yi), &m), _)| {
                let dm = F::d_mean(e);
                if dm.abs() < 1e-300 {
                    e
                } else {
                    e + (yi - m) / dm
                }
            })
            .collect();

        // Build X'WX and X'Wz
        let mut xtwx = vec![vec![0.0_f64; p]; p];
        let mut xtwz = vec![0.0_f64; p];
        for i in 0..n {
            let wi = w[i];
            let zi = z[i];
            for j in 0..p {
                xtwz[j] += wi * x[i][j] * zi;
                for k in j..p {
                    let val = wi * x[i][j] * x[i][k];
                    xtwx[j][k] += val;
                    if j != k {
                        xtwx[k][j] += val;
                    }
                }
            }
        }

        // Solve β_new = (X'WX)⁻¹ X'Wz
        let xtwx_inv = invert_matrix_with_ridge(&xtwx)?;
        let mut beta_new = vec![0.0_f64; p];
        for j in 0..p {
            for k in 0..p {
                beta_new[j] += xtwx_inv[j][k] * xtwz[k];
            }
        }

        // Check convergence: relative change in beta
        let norm_old: f64 = beta.iter().map(|b| b * b).sum::<f64>().sqrt();
        let norm_diff: f64 = beta
            .iter()
            .zip(beta_new.iter())
            .map(|(b, bn)| (b - bn).powi(2))
            .sum::<f64>()
            .sqrt();
        let rel_change = if norm_old > 1e-10 {
            norm_diff / norm_old
        } else {
            norm_diff
        };

        beta = beta_new;

        if rel_change < tol {
            converged = true;
            break;
        }
    }

    if !converged {
        return Err(format!(
            "IRLS did not converge after {max_iter} iterations. Check for separation or collinearity."
        ));
    }

    // Compute final quantities
    let eta: Vec<f64> = x
        .iter()
        .zip(off.iter())
        .map(|(row, &o)| {
            let xb: f64 = row.iter().zip(beta.iter()).map(|(xi, bi)| xi * bi).sum();
            xb + o
        })
        .collect();
    let mu: Vec<f64> = eta.iter().map(|&e| F::mean(e)).collect();

    let deviance: f64 = y
        .iter()
        .zip(mu.iter())
        .map(|(&yi, &mi)| F::deviance_contribution(yi, mi))
        .sum();

    let pearson_chi_square: f64 = y
        .iter()
        .zip(mu.iter())
        .map(|(&yi, &mi)| {
            let v = F::variance(mi);
            if v > 0.0 {
                (yi - mi).powi(2) / v
            } else {
                0.0
            }
        })
        .sum();

    let log_likelihood: f64 = y
        .iter()
        .zip(mu.iter())
        .map(|(&yi, &mi)| F::log_likelihood_contribution(yi, mi))
        .sum();

    // Variance-covariance matrix = (X'WX)⁻¹
    let w_final: Vec<f64> = eta
        .iter()
        .zip(mu.iter())
        .map(|(&e, &m)| {
            let dm = F::d_mean(e);
            let v = F::variance(m);
            if v <= 0.0 || dm == 0.0 {
                0.0
            } else {
                dm * dm / v
            }
        })
        .collect();
    let mut xtwx_final = vec![vec![0.0_f64; p]; p];
    for i in 0..n {
        let wi = w_final[i];
        for j in 0..p {
            for k in j..p {
                let val = wi * x[i][j] * x[i][k];
                xtwx_final[j][k] += val;
                if j != k {
                    xtwx_final[k][j] += val;
                }
            }
        }
    }
    let vcov = invert_matrix_with_ridge(&xtwx_final)?;

    Ok(IrlsFit {
        beta,
        vcov,
        iterations,
        converged,
        deviance,
        pearson_chi_square,
        log_likelihood,
    })
}
