use crate::cli::NaStrategy;
use crate::schema::ClusterResult;

use super::common::{mean, prelude_notes, EPS};
use super::pca::numeric_matrix;

pub(crate) fn cluster_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    vars: &[String],
    k: usize,
    method: &str,
    seed: Option<u64>,
    strategy: NaStrategy,
) -> Result<ClusterResult, String> {
    if k < 2 {
        return Err("Cluster analysis requires k >= 2.".to_string());
    }
    let data = numeric_matrix(rows, headers, vars, strategy)?;
    if data.values.len() < k {
        return Err("Cluster analysis requires at least k complete observations.".to_string());
    }
    if method.eq_ignore_ascii_case("hierarchical") {
        let (assignments, merge_distances, centroids, within_ss) =
            ward_hierarchical(&data.values, k);
        let silhouettes = silhouette_scores(&data.values, &assignments, k);
        let total_within = within_ss.iter().sum();
        return Ok(ClusterResult {
            status: "ok".to_string(),
            data_path: String::new(),
            analysis_path: None,
            n_total: rows.len(),
            n_used: data.n_used,
            n_excluded_missing: data.n_excluded,
            notes: prelude_notes(data.n_used, rows.len(), data.n_excluded),
            warnings: vec![],
            method: "hierarchical".to_string(),
            k,
            variables: vars.to_vec(),
            assignments,
            centroids,
            within_cluster_ss: within_ss,
            total_within_ss: total_within,
            silhouette_avg: if silhouettes.is_empty() {
                f64::NAN
            } else {
                mean(&silhouettes)
            },
            silhouette_per_observation: silhouettes,
            merge_distances,
            excluded_variables: Vec::new(),
        });
    }
    let seed = seed.ok_or_else(|| "k-means requires --seed for reproducibility.".to_string())?;
    let (assignments, centroids, within) = kmeans(&data.values, k, seed);
    let silhouettes = silhouette_scores(&data.values, &assignments, k);
    let total_within_ss = within.iter().sum();
    Ok(ClusterResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: data.n_used,
        n_excluded_missing: data.n_excluded,
        notes: prelude_notes(data.n_used, rows.len(), data.n_excluded),
        warnings: vec![],
        method: "kmeans".to_string(),
        k,
        variables: vars.to_vec(),
        assignments,
        centroids,
        within_cluster_ss: within,
        total_within_ss,
        silhouette_avg: if silhouettes.is_empty() {
            f64::NAN
        } else {
            mean(&silhouettes)
        },
        silhouette_per_observation: silhouettes,
        merge_distances: Vec::new(),
        excluded_variables: Vec::new(),
    })
}

/// Ward hierarchical clustering via Lance-Williams update formula.
///
/// Returns (assignments, `merge_distances`, centroids, `within_ss`).
/// `merge_distances` is the increase in total within-cluster SS at each merge step.
fn ward_hierarchical(
    data: &[Vec<f64>],
    k: usize,
) -> (Vec<usize>, Vec<f64>, Vec<Vec<f64>>, Vec<f64>) {
    let n = data.len();
    let p = data[0].len();
    if n <= k {
        let assignments: Vec<usize> = (0..n).collect();
        let mut padded = assignments.clone();
        padded.resize(n, 0);
        return (padded, Vec::new(), Vec::new(), vec![0.0; k.min(n)]);
    }

    // Compute all pairwise squared Euclidean distances
    let mut dist = vec![vec![f64::INFINITY; n]; n];
    for i in 0..n {
        for j in (i + 1)..n {
            let d = squared_distance(&data[i], &data[j]);
            dist[i][j] = d;
            dist[j][i] = d;
        }
    }

    // Each observation starts as its own cluster
    let mut cluster_sizes = vec![1usize; n];
    let mut cluster_assignments: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    let mut active: Vec<bool> = vec![true; n];
    let mut merge_distances = Vec::new();

    // Merge until only k clusters remain (or 1 if we want all merges)
    let target_clusters = 1usize; // record all merges for the dendrogram
    let mut current_clusters = n;

    while current_clusters > target_clusters {
        // Find the pair of active clusters with minimum Ward distance
        let active_indices: Vec<usize> = (0..n).filter(|&i| active[i]).collect();
        let mut min_dist = f64::INFINITY;
        let mut merge_i = 0;
        let mut merge_j = 0;
        for &a in &active_indices {
            for &b in &active_indices {
                if a >= b {
                    continue;
                }
                let d = dist[a][b];
                if d < min_dist {
                    min_dist = d;
                    merge_i = a;
                    merge_j = b;
                }
            }
        }
        if min_dist >= f64::INFINITY {
            break;
        }

        merge_distances.push(min_dist);

        // Merge cluster j into i
        let n_i = cluster_sizes[merge_i];
        let n_j = cluster_sizes[merge_j];

        // Transfer members
        let members_j = std::mem::take(&mut cluster_assignments[merge_j]);
        cluster_assignments[merge_i].extend(members_j);
        cluster_sizes[merge_i] += n_j;
        cluster_sizes[merge_j] = 0;
        active[merge_j] = false;

        // Update distances from merged cluster to all other active clusters using Lance-Williams
        for &c in &active_indices {
            if c == merge_i || c == merge_j {
                continue;
            }
            let n_c = cluster_sizes[c];
            let denom = (n_i + n_j + n_c) as f64;
            let alpha_i = (n_i + n_c) as f64 / denom;
            let alpha_j = (n_j + n_c) as f64 / denom;
            let beta = -(n_c as f64) / denom;
            let new_dist = alpha_i * dist[merge_i][c]
                + alpha_j * dist[merge_j][c]
                + beta * dist[merge_i][merge_j];
            dist[merge_i][c] = new_dist.max(0.0);
            dist[c][merge_i] = new_dist.max(0.0);
        }
        dist[merge_i][merge_j] = f64::INFINITY;
        dist[merge_j][merge_i] = f64::INFINITY;

        current_clusters -= 1;
    }

    // Collect surviving clusters
    let surviving: Vec<usize> = (0..n).filter(|&i| active[i]).collect();

    // If requested k differs from surviving, prune additional merges
    let final_clusters = if surviving.len() > k {
        // We recorded all merge distances; to get to k clusters, drop the
        // first (n - k) merge distances from the output to reflect only
        // the last (surviving - k) + 1 merges...
        // For simplicity, we always keep all merges and provide assignments for k
        let drain = surviving.len() - k;
        merge_distances.drain(0..drain);
        k
    } else {
        surviving.len()
    };

    // Build assignments: each active cluster gets a label
    let mut assignments = vec![0usize; n];
    for (label, &cluster_idx) in surviving.iter().enumerate().take(final_clusters) {
        for &member in &cluster_assignments[cluster_idx] {
            assignments[member] = label;
        }
    }

    // Compute centroids and within-cluster SS
    let mut centroids = vec![vec![0.0; p]; final_clusters];
    let mut counts = vec![0usize; final_clusters];
    let mut within_ss = vec![0.0; final_clusters];
    for (i, row) in data.iter().enumerate() {
        let c = assignments[i];
        counts[c] += 1;
        for j in 0..p {
            centroids[c][j] += row[j];
        }
    }
    for c in 0..final_clusters {
        if counts[c] > 0 {
            for j in 0..p {
                centroids[c][j] /= counts[c] as f64;
            }
        }
    }
    for (i, row) in data.iter().enumerate() {
        let c = assignments[i];
        within_ss[c] += squared_distance(row, &centroids[c]);
    }

    (assignments, merge_distances, centroids, within_ss)
}

fn kmeans(data: &[Vec<f64>], k: usize, mut seed: u64) -> (Vec<usize>, Vec<Vec<f64>>, Vec<f64>) {
    let p = data[0].len();
    let mut centroids = Vec::new();
    let first = (lcg_next(&mut seed) as usize) % data.len();
    centroids.push(data[first].clone());
    while centroids.len() < k {
        let mut farthest = 0usize;
        let mut farthest_dist = -1.0;
        for (idx, row) in data.iter().enumerate() {
            let dist = centroids
                .iter()
                .map(|c| squared_distance(row, c))
                .fold(f64::INFINITY, f64::min);
            if dist > farthest_dist {
                farthest_dist = dist;
                farthest = idx;
            }
        }
        centroids.push(data[farthest].clone());
    }
    let mut assignments = vec![0usize; data.len()];
    for _ in 0..100 {
        let mut changed = false;
        for (i, row) in data.iter().enumerate() {
            let best = centroids
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    squared_distance(row, a).total_cmp(&squared_distance(row, b))
                })
                .map_or(0, |(idx, _)| idx);
            if assignments[i] != best {
                assignments[i] = best;
                changed = true;
            }
        }
        let mut sums = vec![vec![0.0; p]; k];
        let mut counts = vec![0usize; k];
        for (row, &cluster) in data.iter().zip(assignments.iter()) {
            counts[cluster] += 1;
            for j in 0..p {
                sums[cluster][j] += row[j];
            }
        }
        for cluster in 0..k {
            if counts[cluster] > 0 {
                for j in 0..p {
                    sums[cluster][j] /= counts[cluster] as f64;
                }
                centroids[cluster] = sums[cluster].clone();
            }
        }
        if !changed {
            break;
        }
    }
    let mut within = vec![0.0; k];
    for (row, &cluster) in data.iter().zip(assignments.iter()) {
        within[cluster] += squared_distance(row, &centroids[cluster]);
    }
    (assignments, centroids, within)
}

fn lcg_next(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    *seed
}

fn squared_distance(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum()
}

fn distance(a: &[f64], b: &[f64]) -> f64 {
    squared_distance(a, b).sqrt()
}

fn silhouette_scores(data: &[Vec<f64>], assignments: &[usize], k: usize) -> Vec<f64> {
    let mut scores = Vec::with_capacity(data.len());
    for (i, row) in data.iter().enumerate() {
        let own = assignments[i];
        let mut a_sum = 0.0;
        let mut a_n = 0usize;
        let mut b = f64::INFINITY;
        for cluster in 0..k {
            let mut sum = 0.0;
            let mut count = 0usize;
            for (j, other) in data.iter().enumerate() {
                if i == j || assignments[j] != cluster {
                    continue;
                }
                sum += distance(row, other);
                count += 1;
            }
            if count == 0 {
                continue;
            }
            let avg = sum / count as f64;
            if cluster == own {
                a_sum = sum;
                a_n = count;
            } else {
                b = b.min(avg);
            }
        }
        let a = if a_n > 0 { a_sum / a_n as f64 } else { 0.0 };
        let denom = a.max(b);
        scores.push(if denom.is_finite() && denom > EPS {
            (b - a) / denom
        } else {
            0.0
        });
    }
    scores
}
