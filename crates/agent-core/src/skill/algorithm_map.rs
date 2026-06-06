//! Skill-to-Algorithm Map.
//!
//! A pure, static, total function that resolves a skill id to at most one
//! Output-Level Algorithm id present in the Algorithm Coverage Matrix.
//!
//! The returned ids are verbatim, case-sensitive matrix ids from
//! `coverage_matrix/matrix.toml`. Skills with no entry (non-output-level
//! skills such as `inspect` or `power`) resolve to `None`.
//!
//! The mapping is a compile-time constant so it resolves the same skill id
//! to the same algorithm id on every host at every time (Requirement 2.6).

/// Resolve a skill id to at most one Output-Level Algorithm id present in the
/// Algorithm Coverage Matrix. Returns `None` for skills that are not
/// output-level analyses (e.g. `inspect`, `power`).
///
/// The mapping is a compile-time constant, so it resolves the same skill id to
/// the same algorithm id on every host at every time (Requirement 2.6).
///
/// Each returned value is a case-sensitive exact match of an algorithm id
/// present in the Algorithm Coverage Matrix (Requirement 2.2).
#[must_use]
pub fn skill_to_algorithm(skill_id: &str) -> Option<&'static str> {
    match skill_id {
        "model_linear" => Some("linear"),
        "model_logistic" => Some("logistic"),
        "model_cox" => Some("cox"),
        "survival_km" => Some("kaplan_meier"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_linear_maps_to_linear() {
        assert_eq!(skill_to_algorithm("model_linear"), Some("linear"));
    }

    #[test]
    fn test_model_logistic_maps_to_logistic() {
        assert_eq!(skill_to_algorithm("model_logistic"), Some("logistic"));
    }

    #[test]
    fn test_model_cox_maps_to_cox() {
        assert_eq!(skill_to_algorithm("model_cox"), Some("cox"));
    }

    #[test]
    fn test_survival_km_maps_to_kaplan_meier() {
        assert_eq!(skill_to_algorithm("survival_km"), Some("kaplan_meier"));
    }

    #[test]
    fn test_inspect_returns_none() {
        assert_eq!(skill_to_algorithm("inspect"), None);
    }

    #[test]
    fn test_power_returns_none() {
        assert_eq!(skill_to_algorithm("power"), None);
    }

    #[test]
    fn test_unknown_skill_returns_none() {
        assert_eq!(skill_to_algorithm("nonexistent_skill"), None);
    }

    #[test]
    fn test_empty_string_returns_none() {
        assert_eq!(skill_to_algorithm(""), None);
    }

    #[test]
    fn test_deterministic_repeated_calls() {
        // Same input always yields same output (Requirement 2.6)
        for _ in 0..100 {
            assert_eq!(skill_to_algorithm("model_linear"), Some("linear"));
            assert_eq!(skill_to_algorithm("model_cox"), Some("cox"));
            assert_eq!(skill_to_algorithm("inspect"), None);
        }
    }
}
