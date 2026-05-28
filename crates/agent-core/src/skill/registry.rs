//! Skill registry: registration, lookup, and enumeration of `SkillDescriptors`.
//!
//! Each skill maps an LLM tool-call to a `Stats_Engine` CLI subcommand (or native function).
//! The registry is pre-populated with the minimum set required by R10.5.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::models::skill::SkillResult;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Async handler function signature for native skill invocations.
pub type SkillHandlerFn = Arc<
    dyn Fn(Value) -> Pin<Box<dyn Future<Output = Result<SkillResult, String>> + Send>>
        + Send
        + Sync,
>;

/// How a skill is invoked at runtime.
#[derive(Clone)]
pub enum SkillInvoker {
    /// Invoke via subprocess: `stats-code <subcommand...> --json`.
    StatsCli { subcommand: Vec<String> },
    /// Invoke a native async function directly.
    NativeFn { handler: SkillHandlerFn },
}

/// Metadata describing a single statistical skill.
#[derive(Clone)]
pub struct SkillDescriptor {
    /// Unique identifier, e.g. `"model_linear"`.
    pub skill_id: String,
    /// Human-readable display name, e.g. `"线性回归"`.
    pub display_name: String,
    /// JSON Schema (Draft 2020-12) describing accepted input parameters.
    pub input_schema: Value,
    /// JSON Schema describing the structured output.
    pub output_schema: Value,
    /// How to invoke this skill.
    pub invoker: SkillInvoker,
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Registry of available skills. Supports registration, lookup by ID, and enumeration.
#[derive(Clone, Default)]
pub struct SkillRegistry {
    skills: HashMap<String, SkillDescriptor>,
    /// Insertion-order keys for deterministic iteration.
    order: Vec<String>,
}

impl SkillRegistry {
    /// Create an empty registry.
    #[must_use] 
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a skill descriptor. Overwrites if `skill_id` already exists.
    pub fn register(&mut self, desc: SkillDescriptor) {
        if !self.skills.contains_key(&desc.skill_id) {
            self.order.push(desc.skill_id.clone());
        }
        self.skills.insert(desc.skill_id.clone(), desc);
    }

    /// Look up a skill by its ID.
    #[must_use] 
    pub fn get(&self, skill_id: &str) -> Option<&SkillDescriptor> {
        self.skills.get(skill_id)
    }

    /// Return an iterator over all registered skills in insertion order.
    pub fn list(&self) -> impl Iterator<Item = &SkillDescriptor> {
        self.order.iter().filter_map(|id| self.skills.get(id))
    }

    /// Number of registered skills.
    #[must_use] 
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Whether the registry is empty.
    #[must_use] 
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// Create a registry pre-populated with the minimum skill set (R10.5):
    /// `model_linear`, `model_logistic`, `model_cox`, `survival_km`, power, inspect.
    #[must_use] 
    pub fn with_defaults() -> Self {
        let mut reg = Self::new();

        reg.register(SkillDescriptor {
            skill_id: "model_linear".into(),
            display_name: "线性回归".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "outcome": { "type": "string", "description": "因变量列名" },
                    "predictors": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "自变量列名列表"
                    },
                    "dataset_id": { "type": "string", "description": "数据集 ID" }
                },
                "required": ["outcome", "predictors", "dataset_id"]
            }),
            output_schema: json!({
                "type": "object",
                "properties": {
                    "coefficients": { "type": "array" },
                    "r_squared": { "type": "number" },
                    "adj_r_squared": { "type": "number" },
                    "f_statistic": { "type": "number" },
                    "p_value": { "type": "number" },
                    "aic": { "type": "number" }
                }
            }),
            invoker: SkillInvoker::StatsCli {
                subcommand: vec!["model".into(), "linear".into(), "--json".into()],
            },
        });

        reg.register(SkillDescriptor {
            skill_id: "model_logistic".into(),
            display_name: "Logistic 回归".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "outcome": { "type": "string", "description": "因变量列名（二分类）" },
                    "predictors": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "自变量列名列表"
                    },
                    "dataset_id": { "type": "string", "description": "数据集 ID" }
                },
                "required": ["outcome", "predictors", "dataset_id"]
            }),
            output_schema: json!({
                "type": "object",
                "properties": {
                    "coefficients": { "type": "array" },
                    "odds_ratios": { "type": "array" },
                    "p_values": { "type": "array" },
                    "aic": { "type": "number" },
                    "concordance": { "type": "number" }
                }
            }),
            invoker: SkillInvoker::StatsCli {
                subcommand: vec!["model".into(), "logistic".into(), "--json".into()],
            },
        });

        reg.register(SkillDescriptor {
            skill_id: "model_cox".into(),
            display_name: "Cox 回归".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "time": { "type": "string", "description": "时间变量列名" },
                    "event": { "type": "string", "description": "事件变量列名" },
                    "predictors": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "协变量列名列表"
                    },
                    "dataset_id": { "type": "string", "description": "数据集 ID" }
                },
                "required": ["time", "event", "predictors", "dataset_id"]
            }),
            output_schema: json!({
                "type": "object",
                "properties": {
                    "coefficients": { "type": "array" },
                    "hazard_ratios": { "type": "array" },
                    "p_values": { "type": "array" },
                    "concordance": { "type": "number" },
                    "ph_test": { "type": "object" }
                }
            }),
            invoker: SkillInvoker::StatsCli {
                subcommand: vec!["model".into(), "cox".into(), "--json".into()],
            },
        });

        reg.register(SkillDescriptor {
            skill_id: "survival_km".into(),
            display_name: "Kaplan-Meier 生存分析".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "time": { "type": "string", "description": "时间变量列名" },
                    "event": { "type": "string", "description": "事件变量列名" },
                    "group": {
                        "type": "string",
                        "description": "分组变量列名（可选）"
                    },
                    "dataset_id": { "type": "string", "description": "数据集 ID" }
                },
                "required": ["time", "event", "dataset_id"]
            }),
            output_schema: json!({
                "type": "object",
                "properties": {
                    "survival_table": { "type": "array" },
                    "median_survival": { "type": "number" },
                    "log_rank_p": { "type": "number" }
                }
            }),
            invoker: SkillInvoker::StatsCli {
                subcommand: vec!["survival".into(), "km".into(), "--json".into()],
            },
        });

        reg.register(SkillDescriptor {
            skill_id: "power".into(),
            display_name: "功效分析".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "test_type": {
                        "type": "string",
                        "description": "检验类型（如 ttest, anova, proportion）"
                    },
                    "effect_size": { "type": "number", "description": "效应量" },
                    "alpha": { "type": "number", "description": "显著性水平", "default": 0.05 },
                    "power": { "type": "number", "description": "目标功效", "default": 0.8 },
                    "n": { "type": "integer", "description": "样本量（可选，用于计算功效）" }
                },
                "required": ["test_type", "effect_size"]
            }),
            output_schema: json!({
                "type": "object",
                "properties": {
                    "required_n": { "type": "integer" },
                    "achieved_power": { "type": "number" },
                    "effect_size": { "type": "number" },
                    "alpha": { "type": "number" }
                }
            }),
            invoker: SkillInvoker::StatsCli {
                subcommand: vec!["power".into(), "--json".into()],
            },
        });

        reg.register(SkillDescriptor {
            skill_id: "inspect".into(),
            display_name: "数据探索".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "dataset_id": { "type": "string", "description": "数据集 ID" },
                    "columns": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "要探索的列名列表（可选，默认全部）"
                    }
                },
                "required": ["dataset_id"]
            }),
            output_schema: json!({
                "type": "object",
                "properties": {
                    "row_count": { "type": "integer" },
                    "columns": { "type": "array" },
                    "summary_statistics": { "type": "object" }
                }
            }),
            invoker: SkillInvoker::StatsCli {
                subcommand: vec!["inspect".into(), "--json".into()],
            },
        });

        reg
    }
}

// ---------------------------------------------------------------------------
// Debug impl (SkillInvoker contains Arc<dyn Fn> which is not Debug)
// ---------------------------------------------------------------------------

impl std::fmt::Debug for SkillInvoker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StatsCli { subcommand } => f
                .debug_struct("StatsCli")
                .field("subcommand", subcommand)
                .finish(),
            Self::NativeFn { .. } => f.debug_struct("NativeFn").finish_non_exhaustive(),
        }
    }
}

impl std::fmt::Debug for SkillDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkillDescriptor")
            .field("skill_id", &self.skill_id)
            .field("display_name", &self.display_name)
            .field("invoker", &self.invoker)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_registry_is_empty() {
        let reg = SkillRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        assert!(reg.get("anything").is_none());
        assert_eq!(reg.list().count(), 0);
    }

    #[test]
    fn test_register_and_get() {
        let mut reg = SkillRegistry::new();
        reg.register(SkillDescriptor {
            skill_id: "test_skill".into(),
            display_name: "Test".into(),
            input_schema: json!({"type": "object"}),
            output_schema: json!({"type": "object"}),
            invoker: SkillInvoker::StatsCli {
                subcommand: vec!["test".into()],
            },
        });

        assert_eq!(reg.len(), 1);
        assert!(!reg.is_empty());

        let desc = reg.get("test_skill").unwrap();
        assert_eq!(desc.skill_id, "test_skill");
        assert_eq!(desc.display_name, "Test");
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let reg = SkillRegistry::with_defaults();
        assert!(reg.get("nonexistent_skill").is_none());
    }

    #[test]
    fn test_register_overwrites_existing() {
        let mut reg = SkillRegistry::new();
        reg.register(SkillDescriptor {
            skill_id: "dup".into(),
            display_name: "First".into(),
            input_schema: json!({}),
            output_schema: json!({}),
            invoker: SkillInvoker::StatsCli {
                subcommand: vec![],
            },
        });
        reg.register(SkillDescriptor {
            skill_id: "dup".into(),
            display_name: "Second".into(),
            input_schema: json!({}),
            output_schema: json!({}),
            invoker: SkillInvoker::StatsCli {
                subcommand: vec![],
            },
        });

        assert_eq!(reg.len(), 1);
        assert_eq!(reg.get("dup").unwrap().display_name, "Second");
    }

    #[test]
    fn test_list_returns_insertion_order() {
        let mut reg = SkillRegistry::new();
        for id in ["c", "a", "b"] {
            reg.register(SkillDescriptor {
                skill_id: id.into(),
                display_name: id.into(),
                input_schema: json!({}),
                output_schema: json!({}),
                invoker: SkillInvoker::StatsCli {
                    subcommand: vec![],
                },
            });
        }

        let ids: Vec<&str> = reg.list().map(|d| d.skill_id.as_str()).collect();
        assert_eq!(ids, vec!["c", "a", "b"]);
    }

    #[test]
    fn test_with_defaults_has_minimum_set() {
        let reg = SkillRegistry::with_defaults();
        assert_eq!(reg.len(), 6);

        // All required skills present
        let expected_ids = [
            "model_linear",
            "model_logistic",
            "model_cox",
            "survival_km",
            "power",
            "inspect",
        ];
        for id in &expected_ids {
            assert!(reg.get(id).is_some(), "missing skill: {id}");
        }
    }

    #[test]
    fn test_with_defaults_skills_have_schemas() {
        let reg = SkillRegistry::with_defaults();
        for desc in reg.list() {
            // input_schema must be a JSON object with "type" field
            assert_eq!(
                desc.input_schema.get("type").and_then(|v| v.as_str()),
                Some("object"),
                "skill {} input_schema missing type:object",
                desc.skill_id
            );
            // output_schema must be a JSON object with "type" field
            assert_eq!(
                desc.output_schema.get("type").and_then(|v| v.as_str()),
                Some("object"),
                "skill {} output_schema missing type:object",
                desc.skill_id
            );
        }
    }

    #[test]
    fn test_with_defaults_skills_have_invokers() {
        let reg = SkillRegistry::with_defaults();
        for desc in reg.list() {
            match &desc.invoker {
                SkillInvoker::StatsCli { subcommand } => {
                    assert!(
                        !subcommand.is_empty(),
                        "skill {} has empty subcommand",
                        desc.skill_id
                    );
                    assert!(
                        subcommand.contains(&"--json".to_string()),
                        "skill {} subcommand missing --json flag",
                        desc.skill_id
                    );
                }
                SkillInvoker::NativeFn { .. } => {
                    // NativeFn is also valid
                }
            }
        }
    }

    #[test]
    fn test_with_defaults_display_names_are_nonempty() {
        let reg = SkillRegistry::with_defaults();
        for desc in reg.list() {
            assert!(
                !desc.display_name.is_empty(),
                "skill {} has empty display_name",
                desc.skill_id
            );
        }
    }

    #[test]
    fn test_model_linear_input_schema_has_required_fields() {
        let reg = SkillRegistry::with_defaults();
        let desc = reg.get("model_linear").unwrap();
        let required = desc.input_schema.get("required").unwrap().as_array().unwrap();
        let required_strs: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(required_strs.contains(&"outcome"));
        assert!(required_strs.contains(&"predictors"));
        assert!(required_strs.contains(&"dataset_id"));
    }

    #[test]
    fn test_model_cox_input_schema_has_time_and_event() {
        let reg = SkillRegistry::with_defaults();
        let desc = reg.get("model_cox").unwrap();
        let required = desc.input_schema.get("required").unwrap().as_array().unwrap();
        let required_strs: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(required_strs.contains(&"time"));
        assert!(required_strs.contains(&"event"));
        assert!(required_strs.contains(&"predictors"));
    }
}
