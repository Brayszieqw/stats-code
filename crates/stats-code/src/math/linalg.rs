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
}
