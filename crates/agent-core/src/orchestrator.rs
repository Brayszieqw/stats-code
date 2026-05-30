//! `AgentOrchestrator`: core orchestration logic that ties intent recognition,
//! skill dispatch, and interpretation generation into a unified event stream.
//!
//! Decision path (Property 10):
//! - Multiple skills match intent → `AgentEvent::ChoicePrompt` (ask user to choose)
//! - Unique skill, all required args resolved → `AgentEvent::SkillCall` + `SkillResult` + `Interpretation`
//! - Unique skill, missing required args → `AgentEvent::ChoicePrompt` (collect missing params)
//! - Decision assistant on + `SkillResult` emitted → append at least one `ChoicePrompt`
//!
//! Event stream completeness (Property 13):
//! - Successful skill dispatch produces exactly one `SkillResult` and at least one `Interpretation`
//! - `Interpretation` appears AFTER `SkillResult`

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_stream::Stream;

use crate::models::{
    ChoiceOption, ChoicePrompt, ErrorCode, ErrorPayload, SessionId,
    SessionSettings, SkillResult,
};
use crate::skill::{SkillDescriptor, SkillRegistry, SkillRunner};
use crate::traits::{DatasetStore, LlmProvider, SessionStore};
use crate::traits::llm_provider::{LlmEvent, LlmMessage, LlmRequest, LlmRole};

// ---------------------------------------------------------------------------
// AgentEvent
// ---------------------------------------------------------------------------

/// Events emitted by the orchestrator during message processing.
///
/// These are streamed to the client via SSE. Each variant maps to a distinct
/// SSE `event:` field type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    /// Incremental text token from the LLM.
    TextDelta(String),
    /// A structured choice prompt for the user.
    ChoicePrompt(ChoicePrompt),
    /// Notification that a skill is being invoked.
    SkillCall { skill_id: String, args: Value },
    /// The structured result of a skill execution.
    SkillResult(SkillResult),
    /// AI interpretation/evaluation of the skill result.
    Interpretation(String),
    /// An error occurred during processing.
    Error(ErrorPayload),
    /// The event stream is complete.
    Done,
}

// ---------------------------------------------------------------------------
// Intent recognition result (internal)
// ---------------------------------------------------------------------------

/// Result of LLM-based intent recognition.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct IntentResult {
    /// Matched skill IDs (may be 0, 1, or multiple).
    pub skill_ids: Vec<String>,
    /// Resolved arguments from session context / user message.
    pub resolved_args: Value,
    /// Whether the user message contains an explicit query intent.
    pub has_query_intent: bool,
    /// Free-form text response if no skill matches (e.g., chitchat).
    pub text_response: Option<String>,
}

// ---------------------------------------------------------------------------
// Action (internal decision)
// ---------------------------------------------------------------------------

/// Internal action determined by the orchestrator's decision logic.
#[derive(Debug, Clone)]
pub(crate) enum Action {
    /// Ask the user to choose among multiple skills or provide missing args.
    AskChoice(ChoicePrompt),
    /// Run a specific skill with resolved arguments.
    RunSkill { skill_id: String, args: Value },
    /// Respond with plain text (no skill invocation).
    Respond(String),
    /// An error occurred during decision making.
    Error(ErrorPayload),
}

// ---------------------------------------------------------------------------
// AgentOrchestrator
// ---------------------------------------------------------------------------

/// Core orchestrator that processes user messages and produces event streams.
///
/// Generic over `SessionStore` and `DatasetStore` implementations to allow
/// testing with in-memory stores.
pub struct AgentOrchestrator<S: SessionStore, D: DatasetStore> {
    pub store: S,
    pub data: D,
    pub skills: SkillRegistry,
    pub runner: SkillRunner,
    pub llm: Arc<dyn LlmProvider>,
}

/// User message input to the orchestrator.
#[derive(Debug, Clone)]
pub struct UserMessageInput {
    /// The raw text content of the user message.
    pub text: String,
    /// Session settings snapshot (contains `decision_assistant` flag).
    pub settings: SessionSettings,
}

impl<S: SessionStore, D: DatasetStore> AgentOrchestrator<S, D> {
    /// Create a new orchestrator with all dependencies.
    pub fn new(
        store: S,
        data: D,
        skills: SkillRegistry,
        runner: SkillRunner,
        llm: Arc<dyn LlmProvider>,
    ) -> Self {
        Self {
            store,
            data,
            skills,
            runner,
            llm,
        }
    }

    /// Process a user message and return a stream of `AgentEvent`s.
    ///
    /// Decision path:
    /// 1. Recognize intent via LLM
    /// 2. If multiple skills match → emit `ChoicePrompt` (ask user to pick)
    /// 3. If unique skill but missing required args → emit `ChoicePrompt` (collect params)
    /// 4. If unique skill and all args present → run skill → emit `SkillResult` then `Interpretation`
    /// 5. If `decision_assistant` is on and a `SkillResult` was emitted → append `ChoicePrompt`
    pub async fn handle_user_message(
        &self,
        sid: SessionId,
        msg: UserMessageInput,
    ) -> impl Stream<Item = AgentEvent> {
        let events = self.process_message(sid, msg).await;
        tokio_stream::iter(events)
    }

    /// Internal: process the message and collect all events.
    async fn process_message(
        &self,
        sid: SessionId,
        msg: UserMessageInput,
    ) -> Vec<AgentEvent> {
        let mut events = Vec::new();

        // Step 1: Recognize intent via LLM
        let intent = match self.recognize_intent(&msg.text).await {
            Ok(intent) => intent,
            Err(err) => {
                events.push(AgentEvent::Error(err));
                events.push(AgentEvent::Done);
                return events;
            }
        };

        // Step 2: Decide action based on intent
        let action = self.decide_action(&intent, &msg.settings);

        // Step 3: Execute the decided action
        match action {
            Action::AskChoice(prompt) => {
                events.push(AgentEvent::ChoicePrompt(prompt));
            }
            Action::RunSkill { skill_id, args } => {
                // Emit SkillCall notification
                events.push(AgentEvent::SkillCall {
                    skill_id: skill_id.clone(),
                    args: args.clone(),
                });

                // Execute skill
                match self.execute_skill(sid, &skill_id, args).await {
                    Ok(result) => {
                        // Emit SkillResult (must come before Interpretation per P13)
                        events.push(AgentEvent::SkillResult(result.clone()));

                        // Generate interpretation via LLM
                        let interpretation = self
                            .generate_interpretation(&skill_id, &result)
                            .await;
                        events.push(AgentEvent::Interpretation(interpretation));

                        // If decision_assistant is on, append ChoicePrompt (P10)
                        if msg.settings.decision_assistant {
                            let follow_up = self.generate_follow_up_prompt(&skill_id, &result);
                            events.push(AgentEvent::ChoicePrompt(follow_up));
                        }
                    }
                    Err(err) => {
                        events.push(AgentEvent::Error(err));
                    }
                }
            }
            Action::Respond(text) => {
                events.push(AgentEvent::TextDelta(text));
            }
            Action::Error(err) => {
                events.push(AgentEvent::Error(err));
            }
        }

        events.push(AgentEvent::Done);
        events
    }

    /// Recognize intent by calling the LLM with the user message and skill descriptions.
    async fn recognize_intent(&self, user_text: &str) -> Result<IntentResult, ErrorPayload> {
        let skill_descriptions = self.build_skill_descriptions();

        let system_prompt = format!(
            "你是一个统计分析智能体。根据用户消息识别意图并匹配统计技能。\n\
             可用技能列表：\n{skill_descriptions}\n\n\
             请以 JSON 格式返回：\n\
             {{\"skill_ids\": [匹配的skill_id列表], \"resolved_args\": {{已解析的参数}}, \
             \"has_query_intent\": bool, \"text_response\": \"如无匹配skill则返回文字回复\"}}\n\n\
             规则：\n\
             - 如果用户意图明确对应某个技能，返回该 skill_id\n\
             - 如果可能匹配多个技能，返回所有候选 skill_id\n\
             - 如果用户只是闲聊或询问，返回空 skill_ids 和 text_response\n\
             - resolved_args 中只包含用户消息中明确提到的参数值"
        );

        let request = LlmRequest {
            messages: vec![
                LlmMessage {
                    role: LlmRole::System,
                    content: system_prompt,
                },
                LlmMessage {
                    role: LlmRole::User,
                    content: user_text.to_string(),
                },
            ],
            model: String::new(), // Provider uses its configured model
            max_tokens: Some(1024),
            temperature: Some(0.1),
        };

        let stream = self.llm.chat_stream(request).await.map_err(|e| {
            ErrorPayload::new(
                ErrorCode::LlmUnavailable,
                format!("AI 服务暂时不可用：{e}"),
            )
        })?;

        // Collect all text from the stream
        let full_text = collect_stream_text(stream).await;

        // Parse the JSON response from LLM
        parse_intent_response(&full_text)
    }

    /// Decide what action to take based on the recognized intent.
    ///
    /// Decision table (Property 10):
    /// - Multiple skill IDs match → `AskChoice` (let user pick)
    /// - Single skill, all `required_args` resolved → `RunSkill`
    /// - Single skill, missing some `required_args` → `AskChoice` (collect missing)
    /// - No skill matches → Respond with text
    /// - Decision assistant off + no explicit query → no suggestions
    pub(crate) fn decide_action(
        &self,
        intent: &IntentResult,
        settings: &SessionSettings,
    ) -> Action {
        match intent.skill_ids.len() {
            0 => {
                // No skill match → respond with text
                let text = intent
                    .text_response
                    .clone()
                    .unwrap_or_else(|| {
                        // P10: if decision_assistant is off and no query intent,
                        // respond minimally without suggestions
                        if !settings.decision_assistant && !intent.has_query_intent {
                            "好的。".to_string()
                        } else {
                            "我可以帮您进行统计分析。请告诉我您想做什么分析？".to_string()
                        }
                    });

                Action::Respond(text)
            }
            1 => {
                // Single skill matched
                let skill_id = &intent.skill_ids[0];
                match self.skills.get(skill_id) {
                    Some(desc) => {
                        let missing = find_missing_args(desc, &intent.resolved_args);
                        if missing.is_empty() {
                            // All args present → RunSkill
                            Action::RunSkill {
                                skill_id: skill_id.clone(),
                                args: intent.resolved_args.clone(),
                            }
                        } else {
                            // Missing args → AskChoice to collect them (not error, per P10)
                            let prompt = build_missing_args_prompt(desc, &missing);
                            Action::AskChoice(prompt)
                        }
                    }
                    None => {
                        // Skill not found in registry (shouldn't happen but handle gracefully)
                        Action::Error(ErrorPayload::new(
                            ErrorCode::SkillInvalidArgs,
                            format!("未找到技能：{skill_id}"),
                        ))
                    }
                }
            }
            _ => {
                // Multiple skills match → AskChoice (let user pick)
                let prompt = self.build_skill_choice_prompt(&intent.skill_ids);
                Action::AskChoice(prompt)
            }
        }
    }

    /// Execute a skill via the `SkillRunner`.
    async fn execute_skill(
        &self,
        sid: SessionId,
        skill_id: &str,
        args: Value,
    ) -> Result<SkillResult, ErrorPayload> {
        let desc = self.skills.get(skill_id).ok_or_else(|| {
            ErrorPayload::new(
                ErrorCode::SkillInvalidArgs,
                format!("未找到技能：{skill_id}"),
            )
        })?;

        let dataset_id_val = args.get("dataset_id")
            .or_else(|| args.get("dataset"))
            .ok_or_else(|| {
                ErrorPayload::new(
                    ErrorCode::SkillInvalidArgs,
                    "缺少必要参数 dataset_id".to_string(),
                )
            })?;

        let dataset_id_str = dataset_id_val.as_str().ok_or_else(|| {
            ErrorPayload::new(
                ErrorCode::SkillInvalidArgs,
                "参数 dataset_id 格式必须为字符串".to_string(),
            )
        })?;

        let dataset_id = uuid::Uuid::parse_str(dataset_id_str).map_err(|e| {
            ErrorPayload::new(
                ErrorCode::SkillInvalidArgs,
                format!("参数 dataset_id 不是有效的 UUID: {e}"),
            )
        })?;

        let session = self.store.get(sid).await.map_err(|e| {
            ErrorPayload::new(
                ErrorCode::SkillExecutionFailed,
                format!("获取会话失败：{e}"),
            )
        })?;

        let dataset = session.datasets.iter().find(|d| d.dataset_id == dataset_id).ok_or_else(|| {
            ErrorPayload::new(
                ErrorCode::SkillInvalidArgs,
                format!("在当前会话中未找到 ID 为 {dataset_id} 的数据集"),
            )
        })?;

        let dataset_path = self.data.get_path(sid, dataset_id, &dataset.file_name);

        self.runner
            .run(desc, args, &dataset_path)
            .await
            .map_err(|e| {
                use crate::skill::runner::SkillRunError;
                match e {
                    SkillRunError::Timeout { wall_secs } => ErrorPayload::new(
                        ErrorCode::SkillTimeout,
                        format!("统计任务执行超时（超过 {wall_secs} 秒）"),
                    ),
                    SkillRunError::Oom { max_rss_mib } => ErrorPayload::new(
                        ErrorCode::SkillOom,
                        format!("统计任务内存不足（超过 {max_rss_mib} MB）"),
                    ),
                    SkillRunError::ExecutionFailed {
                        stderr_excerpt, ..
                    } => ErrorPayload::new(
                        ErrorCode::SkillExecutionFailed,
                        format!("统计任务失败：{stderr_excerpt}"),
                    ),
                    SkillRunError::SpawnFailed { reason } => ErrorPayload::new(
                        ErrorCode::SkillExecutionFailed,
                        format!("统计任务启动失败：{reason}"),
                    ),
                }
            })
    }

    /// Generate an interpretation of a skill result via LLM.
    async fn generate_interpretation(
        &self,
        skill_id: &str,
        result: &SkillResult,
    ) -> String {
        let display_name = self
            .skills
            .get(skill_id)
            .map_or(skill_id, |d| d.display_name.as_str());

        let result_json = serde_json::to_string_pretty(&result.payload)
            .unwrap_or_else(|_| "{}".to_string());

        let risk_info = if result.risk_signals.is_empty() {
            String::new()
        } else {
            format!(
                "\n\n检测到的风险信号：{:?}",
                result.risk_signals
            )
        };

        let system_prompt = format!(
            "你是一个统计分析专家。请对以下 {display_name} 的分析结果进行解读。\n\
             要求：\n\
             - 解读必须覆盖所有适用维度：系数估计与显著性、模型整体拟合优度、效应量、\
               置信区间、风险/比值比、模型假设是否满足、检验功效\n\
             - 必须引用具体数值（如 p 值、AIC、HR、置信区间）\n\
             - 如有风险信号，明确标注并给出下一步建议\n\
             - 用中文回答\n\n\
             分析结果：\n{result_json}{risk_info}\n"
        );

        let request = LlmRequest {
            messages: vec![
                LlmMessage {
                    role: LlmRole::System,
                    content: system_prompt,
                },
                LlmMessage {
                    role: LlmRole::User,
                    content: "请解读上述分析结果。".to_string(),
                },
            ],
            model: String::new(),
            max_tokens: Some(2048),
            temperature: Some(0.3),
        };

        match self.llm.chat_stream(request).await {
            Ok(stream) => collect_stream_text(stream).await,
            Err(_) => "解读生成失败，请稍后重试。".to_string(),
        }
    }

    /// Generate a follow-up `ChoicePrompt` after a skill result (decision assistant mode).
    fn generate_follow_up_prompt(
        &self,
        _skill_id: &str,
        result: &SkillResult,
    ) -> ChoicePrompt {
        let mut options = Vec::new();
        let mut recommendation = None;

        // Suggest based on risk signals
        if result.risk_signals.iter().any(|r| {
            matches!(r, crate::models::skill::RiskSignal::PValueAboveAlpha)
        }) {
            options.push(ChoiceOption {
                option_id: "change_model".to_string(),
                text: "尝试其他模型".to_string(),
                explanation: Some("当前结果不显著，可能需要换用其他统计方法".to_string()),
            });
        }

        if result.risk_signals.iter().any(|r| {
            matches!(r, crate::models::skill::RiskSignal::VifTooHigh)
        }) {
            options.push(ChoiceOption {
                option_id: "reduce_vars".to_string(),
                text: "减少自变量".to_string(),
                explanation: Some("存在多重共线性，建议移除部分高相关变量".to_string()),
            });
        }

        if result.risk_signals.iter().any(|r| {
            matches!(r, crate::models::skill::RiskSignal::LowPower)
        }) {
            options.push(ChoiceOption {
                option_id: "increase_sample".to_string(),
                text: "功效分析".to_string(),
                explanation: Some("检验功效不足，建议进行样本量估算".to_string()),
            });
        }

        // Always provide standard follow-up options
        options.push(ChoiceOption {
            option_id: "sensitivity".to_string(),
            text: "做敏感性分析".to_string(),
            explanation: Some("验证结果稳健性".to_string()),
        });

        options.push(ChoiceOption {
            option_id: "add_variables".to_string(),
            text: "补充变量".to_string(),
            explanation: Some("加入更多协变量或交互项".to_string()),
        });

        options.push(ChoiceOption {
            option_id: "done".to_string(),
            text: "结束分析".to_string(),
            explanation: Some("当前结果已满足需求".to_string()),
        });

        // Recommend sensitivity analysis if there are risk signals
        if !result.risk_signals.is_empty() && options.iter().any(|o| o.option_id == "sensitivity") {
            recommendation = Some("sensitivity".to_string());
        }

        ChoicePrompt {
            prompt_id: uuid::Uuid::new_v4(),
            question: "分析已完成。您接下来想：".to_string(),
            options,
            multi_select: false,
            allow_custom_text: true,
            recommendation,
        }
    }

    /// Build a description string of all registered skills for the LLM prompt.
    fn build_skill_descriptions(&self) -> String {
        self.skills
            .list()
            .map(|desc| {
                let required = desc
                    .input_schema
                    .get("required")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                format!(
                    "- {} ({}): 必需参数=[{}]",
                    desc.skill_id, desc.display_name, required
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Build a `ChoicePrompt` for selecting among multiple matching skills.
    fn build_skill_choice_prompt(&self, skill_ids: &[String]) -> ChoicePrompt {
        let options: Vec<ChoiceOption> = skill_ids
            .iter()
            .filter_map(|id| self.skills.get(id))
            .map(|desc| ChoiceOption {
                option_id: desc.skill_id.clone(),
                text: desc.display_name.clone(),
                explanation: Some(format!("使用 {} 进行分析", desc.display_name)),
            })
            .collect();

        ChoicePrompt {
            prompt_id: uuid::Uuid::new_v4(),
            question: "检测到多个可能的分析方法，请选择：".to_string(),
            options,
            multi_select: false,
            allow_custom_text: true,
            recommendation: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Collect all text deltas from an LLM stream into a single string.
async fn collect_stream_text(stream: crate::traits::llm_provider::LlmStream) -> String {
    use tokio_stream::StreamExt;

    let mut text = String::new();
    let mut stream = stream;
    while let Some(event) = stream.next().await {
        match event {
            LlmEvent::TextDelta(delta) => text.push_str(&delta),
            LlmEvent::Done => break,
            LlmEvent::Error(_) => break,
        }
    }
    text
}

/// Parse the LLM's JSON response into an `IntentResult`.
///
/// If parsing fails, returns a default intent with no skill match and
/// uses the raw text as a text response.
fn parse_intent_response(text: &str) -> Result<IntentResult, ErrorPayload> {
    // Try to extract JSON from the response (LLM might wrap it in markdown code blocks)
    let json_str = extract_json_from_text(text);

    match serde_json::from_str::<IntentResult>(json_str) {
        Ok(intent) => Ok(intent),
        Err(_) => {
            // If we can't parse structured intent, treat it as a text response
            Ok(IntentResult {
                skill_ids: vec![],
                resolved_args: Value::Object(serde_json::Map::new()),
                has_query_intent: false,
                text_response: Some(text.to_string()),
            })
        }
    }
}

/// Extract JSON content from text that might be wrapped in markdown code blocks.
fn extract_json_from_text(text: &str) -> &str {
    let text = text.trim();

    // Try to find JSON within ```json ... ``` blocks
    if let Some(start) = text.find("```json") {
        let json_start = start + 7; // skip "```json"
        if let Some(end) = text[json_start..].find("```") {
            return text[json_start..json_start + end].trim();
        }
    }

    // Try to find JSON within ``` ... ``` blocks
    if let Some(start) = text.find("```") {
        let content_start = start + 3;
        if let Some(end) = text[content_start..].find("```") {
            return text[content_start..content_start + end].trim();
        }
    }

    // Try to find JSON object directly
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            if end > start {
                return &text[start..=end];
            }
        }
    }

    text
}

/// Find required args that are missing from the resolved args.
fn find_missing_args(desc: &SkillDescriptor, resolved_args: &Value) -> Vec<String> {
    let required = desc
        .input_schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let resolved_obj = resolved_args.as_object();

    required
        .into_iter()
        .filter(|arg| {
            resolved_obj
                .and_then(|obj| obj.get(arg.as_str()))
                .is_none_or(serde_json::Value::is_null)
        })
        .collect()
}

/// Build a `ChoicePrompt` to collect missing required arguments from the user.
fn build_missing_args_prompt(desc: &SkillDescriptor, missing: &[String]) -> ChoicePrompt {
    let properties = desc
        .input_schema
        .get("properties")
        .and_then(|v| v.as_object());

    let question = format!(
        "执行「{}」还需要以下信息：",
        desc.display_name
    );

    let options: Vec<ChoiceOption> = missing
        .iter()
        .map(|arg| {
            let description = properties
                .and_then(|props| props.get(arg.as_str()))
                .and_then(|schema| schema.get("description"))
                .and_then(|d| d.as_str())
                .map(String::from);

            ChoiceOption {
                option_id: arg.clone(),
                text: description.clone().unwrap_or_else(|| arg.clone()),
                explanation: description,
            }
        })
        .collect();

    ChoicePrompt {
        prompt_id: uuid::Uuid::new_v4(),
        question,
        options,
        multi_select: false,
        allow_custom_text: true,
        recommendation: None,
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::mock::{MockLlm, MockLlmResponse};
    use crate::models::SessionSettings;
    use crate::skill::SkillRegistry;
    use crate::traits::llm_provider::LlmEvent;
    use crate::traits::StoreError;
    use serde_json::json;
    use std::path::PathBuf;
    use tokio_stream::StreamExt;

    // --- Helper: build a mock orchestrator ---

    fn mock_runner() -> SkillRunner {
        SkillRunner::new(
            PathBuf::from("mock-stats-code"),
            PathBuf::from("/tmp"),
            60,
            1024,
        )
    }

    /// A simple in-memory session store for testing.
    struct NullSessionStore;

    #[async_trait::async_trait]
    impl SessionStore for NullSessionStore {
        async fn create(&self) -> Result<crate::models::Session, StoreError> {
            unimplemented!()
        }
        async fn get(&self, _id: SessionId) -> Result<crate::models::Session, StoreError> {
            unimplemented!()
        }
        async fn append_message(
            &self,
            _id: SessionId,
            _msg: crate::models::Message,
        ) -> Result<(), StoreError> {
            Ok(())
        }
        async fn append_skill_run(
            &self,
            _id: SessionId,
            _run: crate::models::SkillRun,
        ) -> Result<(), StoreError> {
            Ok(())
        }
        async fn update_settings(
            &self,
            _id: SessionId,
            _s: SessionSettings,
        ) -> Result<(), StoreError> {
            Ok(())
        }
        async fn archive(&self, _id: SessionId) -> Result<(), StoreError> {
            Ok(())
        }
        async fn touch(&self, _id: SessionId) -> Result<(), StoreError> {
            Ok(())
        }
        async fn list_archivable(
            &self,
            _before: chrono::DateTime<chrono::Utc>,
        ) -> Result<Vec<SessionId>, StoreError> {
            Ok(vec![])
        }
        async fn append_dataset(
            &self,
            _id: SessionId,
            _dataset: crate::models::DatasetSummary,
        ) -> Result<(), StoreError> {
            Ok(())
        }
    }

    /// A null dataset store for testing.
    struct NullDatasetStore;

    #[async_trait::async_trait]
    impl DatasetStore for NullDatasetStore {
        async fn save_raw(
            &self,
            _sid: SessionId,
            _name: &str,
            _bytes: bytes::Bytes,
        ) -> Result<crate::models::DatasetRef, StoreError> {
            unimplemented!()
        }
        async fn parse(
            &self,
            _dref: crate::models::DatasetRef,
        ) -> Result<crate::models::DatasetSummary, StoreError> {
            unimplemented!()
        }
        async fn delete_session_data(&self, _sid: SessionId) -> Result<(), StoreError> {
            Ok(())
        }
        async fn quota_used(&self, _sid: SessionId) -> Result<u64, StoreError> {
            Ok(0)
        }
        fn get_path(
            &self,
            _sid: SessionId,
            _dataset_id: crate::models::DatasetId,
            _name: &str,
        ) -> std::path::PathBuf {
            std::path::PathBuf::new()
        }
    }

    fn make_orchestrator(
        llm: MockLlm,
    ) -> AgentOrchestrator<NullSessionStore, NullDatasetStore> {
        AgentOrchestrator::new(
            NullSessionStore,
            NullDatasetStore,
            SkillRegistry::with_defaults(),
            mock_runner(),
            Arc::new(llm),
        )
    }

    // --- Tests for decide_action ---

    #[test]
    fn test_decide_action_no_skill_match_returns_respond() {
        let orch = make_orchestrator(MockLlm::new(vec![]));
        let intent = IntentResult {
            skill_ids: vec![],
            resolved_args: json!({}),
            has_query_intent: false,
            text_response: Some("你好！有什么可以帮您？".to_string()),
        };
        let settings = SessionSettings::default();

        let action = orch.decide_action(&intent, &settings);
        assert!(matches!(action, Action::Respond(ref t) if t == "你好！有什么可以帮您？"));
    }

    #[test]
    fn test_decide_action_multiple_skills_returns_ask_choice() {
        let orch = make_orchestrator(MockLlm::new(vec![]));
        let intent = IntentResult {
            skill_ids: vec!["model_linear".into(), "model_logistic".into()],
            resolved_args: json!({}),
            has_query_intent: true,
            text_response: None,
        };
        let settings = SessionSettings::default();

        let action = orch.decide_action(&intent, &settings);
        match action {
            Action::AskChoice(prompt) => {
                assert_eq!(prompt.options.len(), 2);
                assert_eq!(prompt.options[0].option_id, "model_linear");
                assert_eq!(prompt.options[1].option_id, "model_logistic");
            }
            _ => panic!("Expected AskChoice, got {:?}", action),
        }
    }

    #[test]
    fn test_decide_action_single_skill_all_args_returns_run_skill() {
        let orch = make_orchestrator(MockLlm::new(vec![]));
        let intent = IntentResult {
            skill_ids: vec!["model_linear".into()],
            resolved_args: json!({
                "outcome": "blood_pressure",
                "predictors": ["age", "weight"],
                "dataset_id": "ds-001"
            }),
            has_query_intent: true,
            text_response: None,
        };
        let settings = SessionSettings::default();

        let action = orch.decide_action(&intent, &settings);
        match action {
            Action::RunSkill { skill_id, args } => {
                assert_eq!(skill_id, "model_linear");
                assert_eq!(args["outcome"], "blood_pressure");
            }
            _ => panic!("Expected RunSkill, got {:?}", action),
        }
    }

    #[test]
    fn test_decide_action_single_skill_missing_args_returns_ask_choice() {
        let orch = make_orchestrator(MockLlm::new(vec![]));
        let intent = IntentResult {
            skill_ids: vec!["model_linear".into()],
            resolved_args: json!({
                "dataset_id": "ds-001"
            }),
            has_query_intent: true,
            text_response: None,
        };
        let settings = SessionSettings::default();

        let action = orch.decide_action(&intent, &settings);
        match action {
            Action::AskChoice(prompt) => {
                // Should ask for missing "outcome" and "predictors"
                assert!(prompt.question.contains("线性回归"));
                assert!(!prompt.options.is_empty());
            }
            _ => panic!("Expected AskChoice for missing args, got {:?}", action),
        }
    }

    #[test]
    fn test_decide_action_unknown_skill_id_returns_error() {
        let orch = make_orchestrator(MockLlm::new(vec![]));
        let intent = IntentResult {
            skill_ids: vec!["nonexistent_skill".into()],
            resolved_args: json!({}),
            has_query_intent: true,
            text_response: None,
        };
        let settings = SessionSettings::default();

        let action = orch.decide_action(&intent, &settings);
        assert!(matches!(action, Action::Error(_)));
    }

    // --- Tests for handle_user_message (async stream) ---

    #[tokio::test]
    async fn test_handle_message_text_response_no_skill() {
        let llm_response = json!({
            "skill_ids": [],
            "resolved_args": {},
            "has_query_intent": false,
            "text_response": "你好！我是统计分析助手。"
        });
        let llm = MockLlm::new(vec![MockLlmResponse::Stream(vec![
            LlmEvent::TextDelta(serde_json::to_string(&llm_response).unwrap()),
            LlmEvent::Done,
        ])]);

        let orch = make_orchestrator(llm);
        let sid = SessionId::new();
        let msg = UserMessageInput {
            text: "你好".to_string(),
            settings: SessionSettings::default(),
        };

        let stream = orch.handle_user_message(sid, msg).await;
        let events: Vec<AgentEvent> = stream.collect().await;

        // Should contain a TextDelta and Done
        assert!(events.iter().any(|e| matches!(e, AgentEvent::TextDelta(_))));
        assert!(events.last().map(|e| matches!(e, AgentEvent::Done)).unwrap_or(false));
    }

    #[tokio::test]
    async fn test_handle_message_multiple_skills_emits_choice_prompt() {
        let llm_response = json!({
            "skill_ids": ["model_linear", "model_logistic"],
            "resolved_args": {},
            "has_query_intent": true,
            "text_response": null
        });
        let llm = MockLlm::new(vec![MockLlmResponse::Stream(vec![
            LlmEvent::TextDelta(serde_json::to_string(&llm_response).unwrap()),
            LlmEvent::Done,
        ])]);

        let orch = make_orchestrator(llm);
        let sid = SessionId::new();
        let msg = UserMessageInput {
            text: "我想做回归分析".to_string(),
            settings: SessionSettings::default(),
        };

        let stream = orch.handle_user_message(sid, msg).await;
        let events: Vec<AgentEvent> = stream.collect().await;

        // Should contain a ChoicePrompt
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::ChoicePrompt(_))));
        assert!(events.last().map(|e| matches!(e, AgentEvent::Done)).unwrap_or(false));
    }

    #[tokio::test]
    async fn test_handle_message_llm_unavailable_emits_error() {
        let llm = MockLlm::new(vec![MockLlmResponse::Error(
            crate::traits::llm_provider::LlmError::Unavailable {
                reason: "network timeout".to_string(),
            },
        )]);

        let orch = make_orchestrator(llm);
        let sid = SessionId::new();
        let msg = UserMessageInput {
            text: "分析一下数据".to_string(),
            settings: SessionSettings::default(),
        };

        let stream = orch.handle_user_message(sid, msg).await;
        let events: Vec<AgentEvent> = stream.collect().await;

        // Should contain an Error event with LlmUnavailable
        let has_error = events.iter().any(|e| {
            matches!(e, AgentEvent::Error(ref p) if p.error_code == ErrorCode::LlmUnavailable)
        });
        assert!(has_error);
        assert!(events.last().map(|e| matches!(e, AgentEvent::Done)).unwrap_or(false));
    }

    // --- Tests for helper functions ---

    #[test]
    fn test_extract_json_from_text_plain() {
        let text = r#"{"skill_ids": [], "resolved_args": {}}"#;
        assert_eq!(extract_json_from_text(text), text);
    }

    #[test]
    fn test_extract_json_from_text_code_block() {
        let text = "Here is the result:\n```json\n{\"skill_ids\": []}\n```\nDone.";
        assert_eq!(extract_json_from_text(text), "{\"skill_ids\": []}");
    }

    #[test]
    fn test_extract_json_from_text_embedded_object() {
        let text = "The intent is {\"skill_ids\": [\"model_linear\"]} based on the message.";
        assert_eq!(
            extract_json_from_text(text),
            "{\"skill_ids\": [\"model_linear\"]}"
        );
    }

    #[test]
    fn test_find_missing_args_all_present() {
        let reg = SkillRegistry::with_defaults();
        let desc = reg.get("model_linear").unwrap();
        let args = json!({
            "outcome": "y",
            "predictors": ["x1"],
            "dataset_id": "ds-1"
        });
        assert!(find_missing_args(desc, &args).is_empty());
    }

    #[test]
    fn test_find_missing_args_some_missing() {
        let reg = SkillRegistry::with_defaults();
        let desc = reg.get("model_linear").unwrap();
        let args = json!({
            "dataset_id": "ds-1"
        });
        let missing = find_missing_args(desc, &args);
        assert!(missing.contains(&"outcome".to_string()));
        assert!(missing.contains(&"predictors".to_string()));
    }

    #[test]
    fn test_find_missing_args_null_values_count_as_missing() {
        let reg = SkillRegistry::with_defaults();
        let desc = reg.get("model_linear").unwrap();
        let args = json!({
            "outcome": null,
            "predictors": ["x1"],
            "dataset_id": "ds-1"
        });
        let missing = find_missing_args(desc, &args);
        assert!(missing.contains(&"outcome".to_string()));
        assert!(!missing.contains(&"predictors".to_string()));
    }

    #[test]
    fn test_parse_intent_response_valid_json() {
        let text = r#"{"skill_ids":["model_linear"],"resolved_args":{"outcome":"y"},"has_query_intent":true,"text_response":null}"#;
        let intent = parse_intent_response(text).unwrap();
        assert_eq!(intent.skill_ids, vec!["model_linear"]);
        assert_eq!(intent.resolved_args["outcome"], "y");
        assert!(intent.has_query_intent);
    }

    #[test]
    fn test_parse_intent_response_invalid_json_fallback() {
        let text = "I don't understand what you're asking.";
        let intent = parse_intent_response(text).unwrap();
        assert!(intent.skill_ids.is_empty());
        assert_eq!(intent.text_response.as_deref(), Some(text));
    }

    #[test]
    fn test_build_missing_args_prompt_contains_field_info() {
        let reg = SkillRegistry::with_defaults();
        let desc = reg.get("model_linear").unwrap();
        let missing = vec!["outcome".to_string(), "predictors".to_string()];
        let prompt = build_missing_args_prompt(desc, &missing);

        assert!(prompt.question.contains("线性回归"));
        assert_eq!(prompt.options.len(), 2);
        assert!(prompt.allow_custom_text);
    }
}
