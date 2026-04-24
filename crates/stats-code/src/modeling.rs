// ---------------------------------------------------------------------------
// Shared modeling types used across logistic, cox, and linear regression modules.
// ---------------------------------------------------------------------------

use crate::schema::is_missing_value;

/// Row-level parse status used by all model data-loading loops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowState {
    Ok,
    Missing,
    Invalid,
}

/// A plan for encoding a single variable in the design matrix.
#[derive(Debug, Clone)]
pub(crate) struct LogisticVariablePlan {
    pub(crate) name: String,
    pub(crate) source_index: usize,
    pub(crate) encoding: LogisticEncoding,
}

impl LogisticVariablePlan {
    pub(crate) fn append_design_values(
        &self,
        raw: &str,
        row: &mut Vec<f64>,
    ) -> Result<(), RowState> {
        let trimmed = raw.trim();
        if is_missing_value(trimmed) {
            return Err(RowState::Missing);
        }
        match &self.encoding {
            LogisticEncoding::Continuous => match trimmed.parse::<f64>() {
                Ok(value) if value.is_finite() => {
                    row.push(value);
                    Ok(())
                }
                _ => Err(RowState::Invalid),
            },
            LogisticEncoding::Dummy { reference, levels } => {
                if trimmed == reference || levels.iter().any(|level| level == trimmed) {
                    row.extend(
                        levels
                            .iter()
                            .map(|level| if level == trimmed { 1.0 } else { 0.0 }),
                    );
                    Ok(())
                } else {
                    Err(RowState::Invalid)
                }
            }
            LogisticEncoding::Omitted { .. } => Ok(()),
        }
    }

    pub(crate) fn warning(&self) -> Option<String> {
        match &self.encoding {
            LogisticEncoding::Omitted { reason } => {
                Some(format!("{} omitted: {reason}", self.name))
            }
            LogisticEncoding::Dummy { .. } | LogisticEncoding::Continuous => None,
        }
    }
}

/// How a categorical variable is encoded in the design matrix.
#[derive(Debug, Clone)]
pub(crate) enum LogisticEncoding {
    Continuous,
    Dummy {
        reference: String,
        levels: Vec<String>,
    },
    Omitted {
        reason: String,
    },
}

/// A single term in the design matrix with metadata.
#[derive(Debug, Clone)]
pub(crate) struct LogisticTermSpec {
    pub(crate) term: String,
    pub(crate) variable: String,
    pub(crate) level: Option<String>,
    pub(crate) reference: Option<String>,
}

/// Result of logistic regression Newton-Raphson fitting.
#[derive(Debug, Clone)]
pub(crate) struct LogisticFit {
    pub(crate) beta: Vec<f64>,
    pub(crate) standard_errors: Vec<f64>,
    pub(crate) iterations: usize,
    pub(crate) converged: bool,
    pub(crate) log_likelihood: f64,
    pub(crate) fitted_probabilities: Vec<f64>,
}

/// Result of Cox proportional hazards partial likelihood fitting.
#[derive(Debug, Clone)]
pub(crate) struct CoxFit {
    pub(crate) beta: Vec<f64>,
    pub(crate) standard_errors: Vec<f64>,
    pub(crate) iterations: usize,
    pub(crate) converged: bool,
    pub(crate) log_partial_likelihood: f64,
}
