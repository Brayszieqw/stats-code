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
    for _ in 0..100 {
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
        if max < 1e-10 || n < 2 {
            break;
        }
        let theta = 0.5 * (a[q][q] - a[p][p]).atan2(2.0 * a[p][q]);
        let c = theta.cos();
        let s = theta.sin();
        for row in a.iter_mut().take(n) {
            let aip = row[p];
            let aiq = row[q];
            row[p] = c * aip - s * aiq;
            row[q] = s * aip + c * aiq;
        }
        for j in 0..n {
            let apj = a[p][j];
            let aqj = a[q][j];
            a[p][j] = c * apj - s * aqj;
            a[q][j] = s * apj + c * aqj;
        }
        for row in v.iter_mut().take(n) {
            let vip = row[p];
            let viq = row[q];
            row[p] = c * vip - s * viq;
            row[q] = s * vip + c * viq;
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
pub(crate) fn matrix_trace(matrix: &[Vec<f64>]) -> f64 {
    (0..matrix.len()).map(|i| matrix[i][i]).sum()
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
