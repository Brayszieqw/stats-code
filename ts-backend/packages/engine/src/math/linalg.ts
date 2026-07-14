// math/linalg.ts — pure linear algebra (Phase 2, task 5.1).
// Transcribed from crates/stats-code/src/math/{mod,linalg}.rs, plus the QR and
// Cholesky decompositions the design mandates (standard algorithms).
//
// Matrices are row-major number[][]. All routines are pure and deterministic.

export type Matrix = number[][];

export function dot(a: readonly number[], b: readonly number[]): number {
  let s = 0;
  for (let i = 0; i < a.length; i += 1) {
    s += a[i]! * b[i]!;
  }
  return s;
}

export function matrixVectorMul(matrix: Matrix, vector: readonly number[]): number[] {
  return matrix.map((row) => dot(row, vector));
}

export function matrixMultiply(left: Matrix, right: Matrix): Matrix {
  if (left.length === 0 || right.length === 0) {
    throw new Error('Matrix multiplication requires non-empty matrices.');
  }
  const inner = right.length;
  if (left.some((row) => row.length !== inner)) {
    throw new Error('Left matrix column count must match right matrix row count.');
  }
  const cols = right[0]!.length;
  if (cols === 0 || right.some((row) => row.length !== cols)) {
    throw new Error('Right matrix must be rectangular and non-empty.');
  }
  const out: Matrix = Array.from({ length: left.length }, () => new Array(cols).fill(0));
  for (let i = 0; i < left.length; i += 1) {
    for (let k = 0; k < inner; k += 1) {
      const lik = left[i]![k]!;
      for (let j = 0; j < cols; j += 1) {
        out[i]![j]! += lik * right[k]![j]!;
      }
    }
  }
  return out;
}

export function transpose(m: Matrix): Matrix {
  if (m.length === 0) return [];
  const rows = m.length;
  const cols = m[0]!.length;
  const out: Matrix = Array.from({ length: cols }, () => new Array(rows).fill(0));
  for (let i = 0; i < rows; i += 1) {
    for (let j = 0; j < cols; j += 1) {
      out[j]![i] = m[i]![j]!;
    }
  }
  return out;
}

/** Gauss-Jordan inversion with partial pivoting; throws on singular. */
export function invertMatrix(matrix: Matrix): Matrix {
  const n = matrix.length;
  if (n === 0 || matrix.some((row) => row.length !== n)) {
    throw new Error('Matrix must be square for inversion.');
  }
  const aug: Matrix = matrix.map((row, i) => {
    const r = row.slice();
    for (let k = 0; k < n; k += 1) r.push(0);
    r[n + i] = 1;
    return r;
  });

  for (let col = 0; col < n; col += 1) {
    let maxRow = col;
    let maxVal = Math.abs(aug[col]![col]!);
    for (let row = col + 1; row < n; row += 1) {
      const v = Math.abs(aug[row]![col]!);
      if (v > maxVal) {
        maxVal = v;
        maxRow = row;
      }
    }
    if (maxVal < 1e-15) {
      throw new Error(`Singular matrix at column ${col} (max pivot ${maxVal.toExponential(2)}).`);
    }
    if (maxRow !== col) {
      const tmp = aug[maxRow]!;
      aug[maxRow] = aug[col]!;
      aug[col] = tmp;
    }
    const pivot = aug[col]![col]!;
    for (let j = 0; j < 2 * n; j += 1) {
      aug[col]![j]! /= pivot;
    }
    for (let row = 0; row < n; row += 1) {
      if (row === col) continue;
      const factor = aug[row]![col]!;
      for (let j = 0; j < 2 * n; j += 1) {
        aug[row]![j]! -= factor * aug[col]![j]!;
      }
    }
  }
  return aug.map((row) => row.slice(n));
}

export interface MatrixInverseResult {
  inverse: Matrix;
  ridgeApplied: boolean;
  ridgeValue: number;
}

/** Matrix inversion with progressive ridge regularization fallback and diagnostics. */
export function invertMatrixWithRidge(matrix: Matrix): MatrixInverseResult {
  try {
    return { inverse: invertMatrix(matrix), ridgeApplied: false, ridgeValue: 0 };
  } catch {
    const n = matrix.length;
    let ridge = 1e-8;
    for (let attempt = 0; attempt < 8; attempt += 1) {
      const reg = matrix.map((row) => row.slice());
      for (let i = 0; i < n; i += 1) {
        reg[i]![i]! += ridge;
      }
      try {
        return { inverse: invertMatrix(reg), ridgeApplied: true, ridgeValue: ridge };
      } catch {
        ridge *= 10;
      }
    }
    throw new Error('Design matrix is singular; check collinearity or constant predictors.');
  }
}

/** Determinant via LU decomposition with partial pivoting. */
export function matrixDeterminant(matrix: Matrix): number {
  const n = matrix.length;
  if (n === 0) return 1;
  if (matrix.some((row) => row.length !== n)) return 0;
  const a = matrix.map((row) => row.slice());
  let det = 1;
  for (let i = 0; i < n; i += 1) {
    let pivot = i;
    let maxVal = Math.abs(a[i]![i]!);
    for (let r = i + 1; r < n; r += 1) {
      const v = Math.abs(a[r]![i]!);
      if (v > maxVal) {
        maxVal = v;
        pivot = r;
      }
    }
    if (maxVal < 1e-15) return 0;
    if (pivot !== i) {
      const tmp = a[i]!;
      a[i] = a[pivot]!;
      a[pivot] = tmp;
      det = -det;
    }
    det *= a[i]![i]!;
    for (let r = i + 1; r < n; r += 1) {
      const factor = a[r]![i]! / a[i]![i]!;
      for (let c = i; c < n; c += 1) {
        a[r]![c]! -= factor * a[i]![c]!;
      }
    }
  }
  return det;
}

export function matrixTrace(matrix: Matrix): number {
  const n = matrix.length;
  if (n === 0 || matrix.some((row) => row.length !== n)) return 0;
  let s = 0;
  for (let i = 0; i < n; i += 1) s += matrix[i]![i]!;
  return s;
}

/**
 * Cholesky decomposition of a symmetric positive-definite matrix A = L·Lᵀ.
 * Returns the lower-triangular L. Throws if A is not positive-definite.
 */
export function cholesky(a: Matrix): Matrix {
  const n = a.length;
  if (n === 0 || a.some((row) => row.length !== n)) {
    throw new Error('Cholesky requires a square matrix.');
  }
  const l: Matrix = Array.from({ length: n }, () => new Array(n).fill(0));
  for (let i = 0; i < n; i += 1) {
    for (let j = 0; j <= i; j += 1) {
      let sum = a[i]![j]!;
      for (let k = 0; k < j; k += 1) {
        sum -= l[i]![k]! * l[j]![k]!;
      }
      if (i === j) {
        if (sum <= 0) {
          throw new Error('Matrix is not positive-definite (Cholesky).');
        }
        l[i]![j] = Math.sqrt(sum);
      } else {
        l[i]![j] = sum / l[j]![j]!;
      }
    }
  }
  return l;
}

/**
 * QR decomposition via the modified Gram-Schmidt process for an m×n matrix
 * (m ≥ n). Returns Q (m×n, orthonormal columns) and R (n×n, upper-triangular)
 * such that A = Q·R.
 */
export function qr(a: Matrix): { q: Matrix; r: Matrix } {
  const m = a.length;
  if (m === 0) {
    return { q: [], r: [] };
  }
  const n = a[0]!.length;
  // Work on column vectors of A.
  const cols: number[][] = [];
  for (let j = 0; j < n; j += 1) {
    cols.push(a.map((row) => row[j]!));
  }
  const q: number[][] = [];
  const r: Matrix = Array.from({ length: n }, () => new Array(n).fill(0));

  for (let j = 0; j < n; j += 1) {
    const v = cols[j]!.slice();
    for (let i = 0; i < j; i += 1) {
      const qi = q[i]!;
      const rij = dot(qi, cols[j]!);
      r[i]![j] = rij;
      for (let k = 0; k < m; k += 1) {
        v[k]! -= rij * qi[k]!;
      }
    }
    let norm = 0;
    for (let k = 0; k < m; k += 1) norm += v[k]! * v[k]!;
    norm = Math.sqrt(norm);
    r[j]![j] = norm;
    if (norm < 1e-15) {
      q.push(new Array(m).fill(0));
    } else {
      q.push(v.map((x) => x / norm));
    }
  }

  // Assemble Q as an m×n matrix (q currently holds n column vectors).
  const qMatrix: Matrix = Array.from({ length: m }, () => new Array(n).fill(0));
  for (let j = 0; j < n; j += 1) {
    for (let i = 0; i < m; i += 1) {
      qMatrix[i]![j] = q[j]![i]!;
    }
  }
  return { q: qMatrix, r };
}

/**
 * Jacobi eigen-decomposition for symmetric matrices. Returns eigenvalues
 * sorted descending and eigenvectors as columns. Transcribed from the Rust
 * jacobi_eigh.
 */
export function jacobiEigh(input: Matrix): { eigenvalues: number[]; eigenvectors: Matrix } {
  const n = input.length;
  const a = input.map((row) => row.slice());
  const v: Matrix = Array.from({ length: n }, () => new Array(n).fill(0));
  for (let i = 0; i < n; i += 1) v[i]![i] = 1;

  for (let sweep = 0; sweep < 200; sweep += 1) {
    let p = 0;
    let q = Math.min(1, Math.max(0, n - 1));
    let max = 0;
    for (let i = 0; i < n; i += 1) {
      for (let j = i + 1; j < n; j += 1) {
        if (Math.abs(a[i]![j]!) > max) {
          max = Math.abs(a[i]![j]!);
          p = i;
          q = j;
        }
      }
    }
    if (max < 1e-12 || n < 2) break;

    const app = a[p]![p]!;
    const aqq = a[q]![q]!;
    const apq = a[p]![q]!;
    const theta =
      Math.abs(app - aqq) < 1e-30
        ? (Math.PI / 4) * Math.sign(apq)
        : 0.5 * Math.atan2(2 * apq, app - aqq);
    const c = Math.cos(theta);
    const s = Math.sin(theta);

    a[p]![p] = c * c * app + s * s * aqq + 2 * c * s * apq;
    a[q]![q] = s * s * app + c * c * aqq - 2 * c * s * apq;
    a[p]![q] = 0;
    a[q]![p] = 0;

    for (let i = 0; i < n; i += 1) {
      if (i === p || i === q) continue;
      const aip = a[i]![p]!;
      const aiq = a[i]![q]!;
      a[i]![p] = c * aip + s * aiq;
      a[p]![i] = a[i]![p]!;
      a[i]![q] = -s * aip + c * aiq;
      a[q]![i] = a[i]![q]!;
    }
    for (let row = 0; row < n; row += 1) {
      const vip = v[row]![p]!;
      const viq = v[row]![q]!;
      v[row]![p] = c * vip + s * viq;
      v[row]![q] = -s * vip + c * viq;
    }
  }

  const pairs = Array.from({ length: n }, (_, i) => ({
    value: Math.max(a[i]![i]!, 0),
    vector: v.map((row) => row[i]!),
  }));
  pairs.sort((l, r) => r.value - l.value);
  const eigenvalues = pairs.map((pr) => pr.value);
  const eigenvectors: Matrix = Array.from({ length: n }, () => new Array(n).fill(0));
  pairs.forEach((pr, col) => {
    for (let row = 0; row < n; row += 1) {
      eigenvectors[row]![col] = pr.vector[row]!;
    }
  });
  return { eigenvalues, eigenvectors };
}
