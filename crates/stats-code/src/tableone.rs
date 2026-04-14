// ---------------------------------------------------------------------------
// Table 1 (baseline characteristics) statistical analysis module.
// ---------------------------------------------------------------------------

use std::collections::BTreeMap;
use std::path::Path;

use crate::cli::TableOneArgs;
use crate::helpers::{{require_column, stringify_error}};
use crate::math::{{chi_square_cdf, kruskal_wallis_test, quantile_sorted, welch_t_pvalue, welch_t_statistic}};
use crate::schema::{{
    infer_variable_kind, is_missing_value, AnalysisSpec,
    TableOneCell, TableOneGroupCell, TableOneResult, TableOneRow, VariableKind, VariableRole,
}};

pub(crate) fn tableone_csv(
    path: &Path,
    analysis_path: Option<&Path>,
    analysis_spec: Option<&AnalysisSpec>,
    args: &TableOneArgs,
) -> Result<TableOneResult, String> {
    let mut reader = csv::Reader::from_path(path).map_err(stringify_error)?;
    let headers = reader
        .headers()
        .map_err(stringify_error)?
        .iter()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let header_index = headers
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let by_index = require_column(&header_index, &args.by)?;
    let selected_variables = resolve_tableone_variables(args, analysis_spec, &headers)?;
    if selected_variables.is_empty() {
        return Err("No variables were selected for Table 1.".to_string());
    }
    let selected_indices = selected_variables
        .iter()
        .map(|name| require_column(&header_index, name).map(|index| (name.clone(), index)))
        .collect::<Result<Vec<_>, _>>()?;

    let mut observations = Vec::new();
    let mut group_levels = BTreeMap::<String, usize>::new();
    let mut skipped_missing_by = 0usize;

    for record in reader.records() {
        let record = record.map_err(stringify_error)?;
        let by_raw = record.get(by_index).unwrap_or_default();
        if is_missing_value(by_raw.trim()) {
            skipped_missing_by += 1;
            continue;
        }

        let group = by_raw.trim().to_string();
        *group_levels.entry(group.clone()).or_insert(0) += 1;
        let values = selected_indices
            .iter()
            .map(|(_, index)| record.get(*index).unwrap_or_default().to_string())
            .collect::<Vec<_>>();
        observations.push(TableOneObservation { group, values });
    }

    if observations.is_empty() {
        return Err(format!(
            "No analyzable rows remained after excluding rows with missing `{}`.",
            args.by
        ));
    }

    let ordered_groups = group_levels.keys().cloned().collect::<Vec<_>>();
    let variable_plans = selected_variables
        .iter()
        .enumerate()
        .map(|(position, name)| TableOneVariablePlan {
            name: name.clone(),
            label: analysis_spec
                .and_then(|spec| {
                    spec.variables
                        .iter()
                        .find(|variable| variable.name == *name)
                })
                .and_then(|variable| variable.label.clone()),
            kind: analysis_spec
                .and_then(|spec| {
                    spec.variables
                        .iter()
                        .find(|variable| variable.name == *name)
                }).map_or_else(|| infer_tableone_kind(name, position, &observations), |variable| variable.kind),
        })
        .collect::<Vec<_>>();

    let mut accumulators = variable_plans
        .iter()
        .map(|plan| TableOneVariableAccumulator::new(plan.kind, &ordered_groups))
        .collect::<Vec<_>>();

    for observation in &observations {
        for (position, value) in observation.values.iter().enumerate() {
            if let Some(accumulator) = accumulators.get_mut(position) {
                accumulator.observe(&observation.group, value);
            }
        }
    }

    let mut rows = Vec::new();
    for (plan, accumulator) in variable_plans.iter().zip(accumulators.iter()) {
        if is_tableone_continuous(plan.kind) {
            let (test_name, p_value) = tableone_continuous_test(accumulator, &ordered_groups);
            rows.push(TableOneRow {
                variable: plan.name.clone(),
                label: plan.label.clone(),
                level: None,
                kind: plan.kind,
                overall: build_continuous_cell(accumulator.overall_continuous()?),
                groups: ordered_groups
                    .iter()
                    .map(|group| {
                        Ok(TableOneGroupCell {
                            group: group.clone(),
                            cell: build_continuous_cell(accumulator.group_continuous(group)?),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?,
                test_name,
                p_value,
                warnings: build_tableone_warnings(accumulator),
            });
            continue;
        }

        let levels = accumulator.levels();
        if levels.is_empty() {
            rows.push(TableOneRow {
                variable: plan.name.clone(),
                label: plan.label.clone(),
                level: None,
                kind: plan.kind,
                overall: empty_categorical_cell(accumulator.overall_categorical()?),
                groups: ordered_groups
                    .iter()
                    .map(|group| {
                        Ok(TableOneGroupCell {
                            group: group.clone(),
                            cell: empty_categorical_cell(accumulator.group_categorical(group)?),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?,
                test_name: None,
                p_value: None,
                warnings: build_tableone_warnings(accumulator),
            });
            continue;
        }

        let (test_name, p_value) = tableone_categorical_test(accumulator, &ordered_groups);
        let mut first_level = true;
        for level in levels {
            rows.push(TableOneRow {
                variable: plan.name.clone(),
                label: plan.label.clone(),
                level: Some(level.clone()),
                kind: plan.kind,
                overall: build_categorical_cell(accumulator.overall_categorical()?, &level),
                groups: ordered_groups
                    .iter()
                    .map(|group| {
                        Ok(TableOneGroupCell {
                            group: group.clone(),
                            cell: build_categorical_cell(
                                accumulator.group_categorical(group)?,
                                &level,
                            ),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?,
                test_name: if first_level { test_name.clone() } else { None },
                p_value: if first_level { p_value } else { None },
                warnings: build_tableone_warnings(accumulator),
            });
            first_level = false;
        }
    }

    Ok(TableOneResult {
        status: "ok".to_string(),
        data_path: path.display().to_string(),
        analysis_path: analysis_path.map(|path| path.display().to_string()),
        by: args.by.clone(),
        group_levels: ordered_groups.clone(),
        rows,
        notes: vec![
            format!(
                "Rows with missing `{}` excluded from grouped summaries: {skipped_missing_by}.",
                args.by
            ),
            format!("Variables summarized: {}.", selected_variables.join(", ")),
            "Continuous variables are shown as mean (SD); median [Q1, Q3].".to_string(),
            "Categorical variables are shown as n (% among non-missing within each column)."
                .to_string(),
        ],
    })
}

pub(crate) fn resolve_tableone_variables(
    args: &TableOneArgs,
    analysis_spec: Option<&AnalysisSpec>,
    headers: &[String],
) -> Result<Vec<String>, String> {
    if !args.vars.is_empty() {
        return Ok(args
            .vars
            .iter()
            .filter(|name| *name != &args.by)
            .cloned()
            .collect());
    }

    if let Some(spec) = analysis_spec {
        let selected = spec
            .variables
            .iter()
            .filter(|variable| variable.name != args.by)
            .filter(|variable| {
                !variable.roles.iter().any(|role| {
                    matches!(
                        role,
                        VariableRole::Outcome
                            | VariableRole::Event
                            | VariableRole::Time
                            | VariableRole::Id
                            | VariableRole::Weight
                            | VariableRole::Cluster
                    )
                })
            })
            .map(|variable| variable.name.clone())
            .collect::<Vec<_>>();
        if !selected.is_empty() {
            return Ok(selected);
        }
    }

    let fallback = headers
        .iter()
        .filter(|name| *name != &args.by)
        .cloned()
        .collect::<Vec<_>>();
    if fallback.is_empty() {
        return Err(
            "No baseline variables were available after excluding the grouping column.".to_string(),
        );
    }
    Ok(fallback)
}

pub(crate) fn infer_tableone_kind(
    name: &str,
    position: usize,
    observations: &[TableOneObservation],
) -> VariableKind {
    let mut distinct_values = std::collections::BTreeSet::new();
    let mut non_missing_count = 0usize;
    let mut numeric_non_missing_count = 0usize;

    for observation in observations {
        if let Some(value) = observation.values.get(position) {
            let trimmed = value.trim();
            if is_missing_value(trimmed) {
                continue;
            }
            non_missing_count += 1;
            if trimmed.parse::<f64>().is_ok() {
                numeric_non_missing_count += 1;
            }
            if distinct_values.len() < 128 {
                distinct_values.insert(trimmed.to_string());
            }
        }
    }

    infer_variable_kind(
        name,
        non_missing_count,
        numeric_non_missing_count,
        &distinct_values,
    )
}

pub(crate) fn is_tableone_continuous(kind: VariableKind) -> bool {
    matches!(
        kind,
        VariableKind::Continuous | VariableKind::Time | VariableKind::PersonTime
    )
}

pub(crate) fn build_tableone_warnings(accumulator: &TableOneVariableAccumulator) -> Vec<String> {
    let total = accumulator.total_records();
    let missing = accumulator.missing_count();
    let mut warnings = Vec::new();
    if total > 0 && (missing as f64 / total as f64) >= 0.2 {
        warnings.push(format!(
            "high_missingness={:.1}%",
            (missing as f64 / total as f64) * 100.0
        ));
    }
    let invalid = accumulator.invalid_count();
    if invalid > 0 {
        warnings.push(format!("non_numeric_treated_as_missing={invalid}"));
    }
    if accumulator.non_missing_count() == 0 {
        warnings.push("no_non_missing_values".to_string());
    }
    warnings
}

pub(crate) fn build_continuous_cell(accumulator: &ContinuousAccumulator) -> TableOneCell {
    let n = accumulator.values.len();
    if n == 0 {
        return TableOneCell {
            display: "NA".to_string(),
            n_total: accumulator.total_records,
            n_non_missing: 0,
            missing_count: accumulator.missing_count,
            count: None,
            percent: None,
            mean: None,
            sd: None,
            median: None,
            q1: None,
            q3: None,
        };
    }

    let mut sorted = accumulator.values.clone();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let mean = accumulator.sum / n as f64;
    let variance = if n > 1 {
        ((accumulator.sum_sq - accumulator.sum.powi(2) / n as f64) / (n - 1) as f64).max(0.0)
    } else {
        0.0
    };
    let sd = variance.sqrt();
    let median = quantile_sorted(&sorted, 0.5);
    let q1 = quantile_sorted(&sorted, 0.25);
    let q3 = quantile_sorted(&sorted, 0.75);

    TableOneCell {
        display: format!("{mean:.2} ({sd:.2}); median {median:.2} [{q1:.2}, {q3:.2}]"),
        n_total: accumulator.total_records,
        n_non_missing: n,
        missing_count: accumulator.missing_count,
        count: None,
        percent: None,
        mean: Some(mean),
        sd: Some(sd),
        median: Some(median),
        q1: Some(q1),
        q3: Some(q3),
    }
}

pub(crate) fn build_categorical_cell(accumulator: &CategoricalAccumulator, level: &str) -> TableOneCell {
    let count = accumulator.counts.get(level).copied().unwrap_or(0);
    let denominator = accumulator.non_missing_count();
    let percent = if denominator == 0 {
        0.0
    } else {
        count as f64 / denominator as f64 * 100.0
    };
    TableOneCell {
        display: format!("{count} ({percent:.1}%)"),
        n_total: accumulator.total_records,
        n_non_missing: denominator,
        missing_count: accumulator.missing_count,
        count: Some(count),
        percent: Some(percent),
        mean: None,
        sd: None,
        median: None,
        q1: None,
        q3: None,
    }
}

pub(crate) fn empty_categorical_cell(accumulator: &CategoricalAccumulator) -> TableOneCell {
    TableOneCell {
        display: "NA".to_string(),
        n_total: accumulator.total_records,
        n_non_missing: accumulator.non_missing_count(),
        missing_count: accumulator.missing_count,
        count: None,
        percent: None,
        mean: None,
        sd: None,
        median: None,
        q1: None,
        q3: None,
    }
}

/// Two-sample Welch t-test (2 groups) or Kruskal-Wallis (>2 groups).
pub(crate) fn tableone_continuous_test(
    accumulator: &TableOneVariableAccumulator,
    ordered_groups: &[String],
) -> (Option<String>, Option<f64>) {
    if ordered_groups.len() < 2 {
        return (None, None);
    }
    let group_values: Vec<&[f64]> = ordered_groups
        .iter()
        .filter_map(|group| {
            accumulator
                .groups
                .get(group)
                .and_then(|acc| acc.as_continuous().ok())
                .map(|cont| cont.values.as_slice())
        })
        .collect();
    if group_values.len() < 2 || group_values.iter().any(|v| v.len() < 2) {
        return (None, None);
    }
    if ordered_groups.len() == 2 {
        let (t_stat, df) = welch_t_statistic(group_values[0], group_values[1]);
        let p = welch_t_pvalue(t_stat, df);
        (Some("Welch_t_test".to_string()), Some(p))
    } else {
        let p = kruskal_wallis_test(&group_values);
        (Some("Kruskal_Wallis".to_string()), p)
    }
}

/// Pearson chi-square test for categorical variables.
pub(crate) fn tableone_categorical_test(
    accumulator: &TableOneVariableAccumulator,
    ordered_groups: &[String],
) -> (Option<String>, Option<f64>) {
    if ordered_groups.len() < 2 {
        return (None, None);
    }
    let group_cats: Vec<&BTreeMap<String, usize>> = ordered_groups
        .iter()
        .filter_map(|group| {
            accumulator
                .groups
                .get(group)
                .and_then(|acc| acc.as_categorical().ok())
                .map(|cat| &cat.counts)
        })
        .collect();
    if group_cats.len() < 2 {
        return (None, None);
    }
    // Collect all levels
    let mut all_levels: Vec<String> = Vec::new();
    for counts in &group_cats {
        for key in counts.keys() {
            if !all_levels.contains(key) {
                all_levels.push(key.clone());
            }
        }
    }
    if all_levels.len() < 2 {
        return (None, None);
    }
    // Build observed matrix: groups x levels
    let observed: Vec<Vec<f64>> = group_cats
        .iter()
        .map(|counts| {
            all_levels
                .iter()
                .map(|level| *counts.get(level).unwrap_or(&0) as f64)
                .collect()
        })
        .collect();
    let n_groups = observed.len();
    let n_levels = all_levels.len();
    let row_totals: Vec<f64> = observed.iter().map(|row| row.iter().sum()).collect();
    let col_totals: Vec<f64> = (0..n_levels)
        .map(|j| observed.iter().map(|row| row[j]).sum())
        .collect();
    let grand_total: f64 = row_totals.iter().sum();
    if grand_total <= 0.0 {
        return (None, None);
    }
    let mut chi2 = 0.0_f64;
    for (i, row) in observed.iter().enumerate() {
        for (j, &obs) in row.iter().enumerate() {
            let expected = row_totals[i] * col_totals[j] / grand_total;
            if expected > 0.0 {
                chi2 += (obs - expected).powi(2) / expected;
            }
        }
    }
    let df = ((n_groups - 1) * (n_levels - 1)) as f64;
    if df <= 0.0 {
        return (None, None);
    }
    let p = 1.0 - chi_square_cdf(chi2, df);
    (Some("Pearson_chi2".to_string()), Some(p))
}

#[derive(Debug, Clone)]
pub(crate) struct TableOneObservation {
    group: String,
    values: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct TableOneVariablePlan {
    name: String,
    label: Option<String>,
    kind: VariableKind,
}

#[derive(Debug, Clone)]
pub(crate) struct TableOneVariableAccumulator {
    overall: TableOneAccumulator,
    groups: BTreeMap<String, TableOneAccumulator>,
}

impl TableOneVariableAccumulator {
    fn new(kind: VariableKind, group_levels: &[String]) -> Self {
        Self {
            overall: TableOneAccumulator::new(kind),
            groups: group_levels
                .iter()
                .map(|group| (group.clone(), TableOneAccumulator::new(kind)))
                .collect(),
        }
    }

    pub(crate) fn observe(&mut self, group: &str, raw: &str) {
        self.overall.observe(raw);
        if let Some(accumulator) = self.groups.get_mut(group) {
            accumulator.observe(raw);
        }
    }

    pub(crate) fn overall_continuous(&self) -> Result<&ContinuousAccumulator, String> {
        self.overall.as_continuous()
    }

    pub(crate) fn overall_categorical(&self) -> Result<&CategoricalAccumulator, String> {
        self.overall.as_categorical()
    }

    pub(crate) fn group_continuous(&self, group: &str) -> Result<&ContinuousAccumulator, String> {
        self.groups
            .get(group)
            .ok_or_else(|| format!("Group `{group}` was not initialized."))?
            .as_continuous()
    }

    pub(crate) fn group_categorical(&self, group: &str) -> Result<&CategoricalAccumulator, String> {
        self.groups
            .get(group)
            .ok_or_else(|| format!("Group `{group}` was not initialized."))?
            .as_categorical()
    }

    pub(crate) fn total_records(&self) -> usize {
        self.overall.total_records()
    }

    pub(crate) fn non_missing_count(&self) -> usize {
        self.overall.non_missing_count()
    }

    pub(crate) fn missing_count(&self) -> usize {
        self.overall.missing_count()
    }

    pub(crate) fn invalid_count(&self) -> usize {
        self.overall.invalid_count()
    }

    pub(crate) fn levels(&self) -> Vec<String> {
        self.overall
            .level_names()
            .unwrap_or_default()
            .into_iter()
            .collect()
    }
}

#[derive(Debug, Clone)]
pub(crate) enum TableOneAccumulator {
    Continuous(ContinuousAccumulator),
    Categorical(CategoricalAccumulator),
}

impl TableOneAccumulator {
    fn new(kind: VariableKind) -> Self {
        if is_tableone_continuous(kind) {
            Self::Continuous(ContinuousAccumulator::default())
        } else {
            Self::Categorical(CategoricalAccumulator::default())
        }
    }

    pub(crate) fn observe(&mut self, raw: &str) {
        match self {
            Self::Continuous(accumulator) => accumulator.observe(raw),
            Self::Categorical(accumulator) => accumulator.observe(raw),
        }
    }

    pub(crate) fn as_continuous(&self) -> Result<&ContinuousAccumulator, String> {
        match self {
            Self::Continuous(accumulator) => Ok(accumulator),
            Self::Categorical(_) => Err("Expected continuous accumulator.".to_string()),
        }
    }

    pub(crate) fn as_categorical(&self) -> Result<&CategoricalAccumulator, String> {
        match self {
            Self::Categorical(accumulator) => Ok(accumulator),
            Self::Continuous(_) => Err("Expected categorical accumulator.".to_string()),
        }
    }

    pub(crate) fn total_records(&self) -> usize {
        match self {
            Self::Continuous(accumulator) => accumulator.total_records,
            Self::Categorical(accumulator) => accumulator.total_records,
        }
    }

    pub(crate) fn non_missing_count(&self) -> usize {
        match self {
            Self::Continuous(accumulator) => accumulator.values.len(),
            Self::Categorical(accumulator) => accumulator.non_missing_count(),
        }
    }

    pub(crate) fn missing_count(&self) -> usize {
        match self {
            Self::Continuous(accumulator) => accumulator.missing_count,
            Self::Categorical(accumulator) => accumulator.missing_count,
        }
    }

    pub(crate) fn invalid_count(&self) -> usize {
        match self {
            Self::Continuous(accumulator) => accumulator.invalid_count,
            Self::Categorical(_) => 0,
        }
    }

    pub(crate) fn level_names(&self) -> Option<Vec<String>> {
        match self {
            Self::Categorical(accumulator) => Some(accumulator.counts.keys().cloned().collect()),
            Self::Continuous(_) => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ContinuousAccumulator {
    total_records: usize,
    missing_count: usize,
    invalid_count: usize,
    sum: f64,
    sum_sq: f64,
    values: Vec<f64>,
}

impl ContinuousAccumulator {
    pub(crate) fn observe(&mut self, raw: &str) {
        self.total_records += 1;
        let trimmed = raw.trim();
        if is_missing_value(trimmed) {
            self.missing_count += 1;
            return;
        }
        match trimmed.parse::<f64>() {
            Ok(value) if value.is_finite() => {
                self.sum += value;
                self.sum_sq += value * value;
                self.values.push(value);
            }
            _ => {
                self.missing_count += 1;
                self.invalid_count += 1;
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CategoricalAccumulator {
    total_records: usize,
    missing_count: usize,
    counts: BTreeMap<String, usize>,
}

impl CategoricalAccumulator {
    pub(crate) fn observe(&mut self, raw: &str) {
        self.total_records += 1;
        let trimmed = raw.trim();
        if is_missing_value(trimmed) {
            self.missing_count += 1;
            return;
        }
        *self.counts.entry(trimmed.to_string()).or_insert(0) += 1;
    }

    pub(crate) fn non_missing_count(&self) -> usize {
        self.counts.values().sum()
    }
}

