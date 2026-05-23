//! Small deterministic linear algebra helpers.

#![allow(dead_code)]

use super::invert_matrix_with_ridge;

/// Matrix multiplication `left * right`.
pub(crate) fn matrix_multiply(
    left: &[Vec<f64>],
    right: &[Vec<f64>],
) -> Result<Vec<Vec<f64>>, String> {
    if left.is_empty() || right.is_empty() {
        return Err("Matrix multiplication requires non-empty matrices.".to_string());
    }
    let inner = right.len();
    if left.iter().any(|row| row.len() != inner) {
        return Err("Left matrix column count must match right matrix row count.".to_string());
    }
    let cols = right[0].len();
    if cols == 0 || right.iter().any(|row| row.len() != cols) {
        return Err("Right matrix must be rectangular and non-empty.".to_string());
    }
    let mut out = vec![vec![0.0; cols]; left.len()];
    for i in 0..left.len() {
        for k in 0..inner {
            let lik = left[i][k];
            for j in 0..cols {
                out[i][j] += lik * right[k][j];
            }
        }
    }
    Ok(out)
}

/// Matrix inversion with progressive ridge fallback.
pub(crate) fn invert_with_ridge(matrix: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, String> {
    invert_matrix_with_ridge(matrix)
}

/// Jacobi eigen-decomposition for symmetric matrices.
///
/// Returns eigenvalues sorted descending and eigenvectors as columns.
pub(crate) fn jacobi_eigh(mut a: Vec<Vec<f64>>) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = a.len();
    let mut v = vec![vec![0.0; n]; n];
    for i in 0..n {
        v[i][i] = 1.0;
    }
    for _ in 0..200 {
        // Find off-diagonal element with largest absolute value.
        let mut p = 0usize;
        let mut q = 1usize.min(n.saturating_sub(1));
        let mut max = 0.0;
        for i in 0..n {
            for j in (i + 1)..n {
                if a[i][j].abs() > max {
                    max = a[i][j].abs();
                    p = i;
                    q = j;
                }
            }
        }
        if max < 1e-12 || n < 2 {
            break;
        }
        // Compute the Jacobi rotation angle.
        // tan(2θ) = 2·a[p][q] / (a[p][p] - a[q][q])
        let app = a[p][p];
        let aqq = a[q][q];
        let apq = a[p][q];
        let theta = if (app - aqq).abs() < 1e-30 {
            std::f64::consts::FRAC_PI_4 * apq.signum()
        } else {
            0.5 * (2.0 * apq).atan2(app - aqq)
        };
        let c = theta.cos();
        let s = theta.sin();

        // Apply the similarity transformation A' = R^T A R.
        // First update the four affected entries of A explicitly to avoid
        // numerical drift.
        let new_app = c * c * app + s * s * aqq + 2.0 * c * s * apq;
        let new_aqq = s * s * app + c * c * aqq - 2.0 * c * s * apq;
        a[p][p] = new_app;
        a[q][q] = new_aqq;
        a[p][q] = 0.0;
        a[q][p] = 0.0;

        // Update remaining entries in rows/columns p and q.
        for i in 0..n {
            if i == p || i == q {
                continue;
            }
            let aip = a[i][p];
            let aiq = a[i][q];
            a[i][p] = c * aip + s * aiq;
            a[p][i] = a[i][p];
            a[i][q] = -s * aip + c * aiq;
            a[q][i] = a[i][q];
        }

        // Update eigenvector matrix V <- V * R, where R has columns
        // [c, s] and [-s, c].
        for row in v.iter_mut().take(n) {
            let vip = row[p];
            let viq = row[q];
            row[p] = c * vip + s * viq;
            row[q] = -s * vip + c * viq;
        }
    }
    let mut pairs: Vec<(f64, Vec<f64>)> = (0..n)
        .map(|i| {
            (
                a[i][i].max(0.0),
                v.iter().map(|row| row[i]).collect::<Vec<_>>(),
            )
        })
        .collect();
    pairs.sort_by(|left, right| right.0.total_cmp(&left.0));
    let eigenvalues = pairs.iter().map(|(value, _)| *value).collect::<Vec<_>>();
    let mut eigenvectors = vec![vec![0.0; n]; n];
    for (col, (_, vector)) in pairs.iter().enumerate() {
        for row in 0..n {
            eigenvectors[row][col] = vector[row];
        }
    }
    (eigenvalues, eigenvectors)
}

/// Determinant of a square matrix via LU-like decomposition with partial pivoting.
pub(crate) fn matrix_determinant(matrix: &[Vec<f64>]) -> f64 {
    let n = matrix.len();
    if n == 0 {
        return 1.0;
    }
    if matrix.iter().any(|row| row.len() != n) {
        return 0.0;
    }
    let mut a = matrix.to_vec();
    let mut det = 1.0;
    for i in 0..n {
        let mut pivot = i;
        let mut max_val = a[i][i].abs();
        for r in (i + 1)..n {
            let val = a[r][i].abs();
            if val > max_val {
                max_val = val;
                pivot = r;
            }
        }
        if max_val < 1e-15 {
            return 0.0;
        }
        if pivot != i {
            a.swap(i, pivot);
            det = -det;
        }
        det *= a[i][i];
        for r in (i + 1)..n {
            let factor = a[r][i] / a[i][i];
            for c in i..n {
                a[r][c] -= factor * a[i][c];
            }
        }
    }
    det
}

/// Matrix trace (sum of diagonal entries).
/// Returns 0.0 for empty or non-square matrices.
pub(crate) fn matrix_trace(matrix: &[Vec<f64>]) -> f64 {
    let n = matrix.len();
    if n == 0 || matrix.iter().any(|row| row.len() != n) {
        return 0.0;
    }
    (0..n).map(|i| matrix[i][i]).sum()
}

/// Compute Helmert contrast matrix of size (p-1) x p.
/// For repeated-measures ANOVA sphericity diagnostics.
pub(crate) fn helmert_contrast_matrix(p: usize) -> Vec<Vec<f64>> {
    let mut c = vec![vec![0.0; p]; p - 1];
    for i in 0..(p - 1) {
        let k = (i + 1) as f64;
        let scale = 1.0 / (k * (k + 1.0)).sqrt();
        for j in 0..=i {
            c[i][j] = -scale;
        }
        c[i][i + 1] = scale * k;
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_multiply_basic() {
        let out = matrix_multiply(
            &[vec![1.0, 2.0], vec![3.0, 4.0]],
            &[vec![5.0, 6.0], vec![7.0, 8.0]],
        )
        .unwrap();
        assert_eq!(out, vec![vec![19.0, 22.0], vec![43.0, 50.0]]);
    }

    #[test]
    fn jacobi_eigh_diagonal_matrix() {
        let (values, vectors) = jacobi_eigh(vec![vec![2.0, 0.0], vec![0.0, 1.0]]);
        assert_eq!(values, vec![2.0, 1.0]);
        assert!((vectors[0][0].abs() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn jacobi_eigh_symmetric_2x2() {
        // A = [[4, 1], [1, 3]]; characteristic eq: λ² - 7λ + 11 = 0
        // λ = (7 ± √5)/2 ≈ 4.618 and 2.382
        let (values, _) = jacobi_eigh(vec![vec![4.0, 1.0], vec![1.0, 3.0]]);
        assert!((values[0] - 4.618_033_988_749_895).abs() < 1e-10);
        assert!((values[1] - 2.381_966_011_250_105).abs() < 1e-10);
    }

    #[test]
    fn jacobi_eigh_correlation_3x3() {
        // Correlation matrix with structure: variables x1, x2, x3 highly correlated.
        // R = [[1, 0.95, 0.93], [0.95, 1, 0.96], [0.93, 0.96, 1]].
        // Trace = 3; first eigenvalue should dominate (~2.92).
        let (values, _) = jacobi_eigh(vec![
            vec![1.0, 0.95, 0.93],
            vec![0.95, 1.0, 0.96],
            vec![0.93, 0.96, 1.0],
        ]);
        let total: f64 = values.iter().sum();
        assert!((total - 3.0).abs() < 1e-9, "trace should be preserved: got {total}");
        assert!(
            values[0] / total > 0.9,
            "first eigenvalue should explain > 90% of variance, got {}",
            values[0] / total
        );
    }

    #[test]
    fn invert_with_ridge_inverts_identity() {
        let inv = invert_with_ridge(&[vec![1.0, 0.0], vec![0.0, 1.0]]).unwrap();
        assert_eq!(inv, vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
    }

    #[test]
    fn determinant_2x2() {
        let det = matrix_determinant(&[vec![2.0, 1.0], vec![1.0, 2.0]]);
        assert!((det - 3.0).abs() < 1e-12);
    }

    #[test]
    fn determinant_identity() {
        let det = matrix_determinant(&[vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0], vec![0.0, 0.0, 1.0]]);
        assert!((det - 1.0).abs() < 1e-12);
    }

    #[test]
    fn trace_3x3() {
        let t = matrix_trace(&[vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0], vec![7.0, 8.0, 9.0]]);
        assert!((t - 15.0).abs() < 1e-12);
    }

    #[test]
    fn helmert_contrast_for_4_timepoints() {
        let c = helmert_contrast_matrix(4);
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].len(), 4);
        // Check orthogonality: each contrast row should sum to zero
        for row in &c {
            let sum: f64 = row.iter().sum();
            assert!(sum.abs() < 1e-12, "helmert row not contrast: sum={sum}");
        }
    }
}
