//! LLM 配置查询 handler 与纯函数。
//!
//! 当前仅落地 task 10.3 的范围：
//! - 类型 [`LlmProvider`] / [`LlmConfig`] / [`LlmStatusResponse`]
//! - 纯函数 [`status_from_config`]
//! - trait [`LlmConfigStore`] 与 `GET /api/llm-status` handler (task 10.6)
//!
//! 设计要点：
//! - `agent-server` crate 不能反向依赖 `stats-code`（后者依赖前者作为库），
//!   因此 LLM 配置契约下沉到 `agent-core::models::llm_config`，launcher 与
//!   HTTP handler 共享同一组 DTO。
//! - 序列化使用 `#[serde(rename_all = "lowercase")]`，对应 R10.3 的契约
//!   `{configured: true, provider: "deepseek" | "openai"}`。

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

pub use agent_core::models::llm_config::{LlmConfig, LlmProvider};

/// 配置存储抽象层。
///
/// launcher 注入 `TomlFileStore` 实例（`%APPDATA%\stats-code\config.toml`）；
/// 测试注入内存实现。
pub trait LlmConfigStore: Send + Sync {
    /// 读取当前配置。返回 None 表示未配置或文件不存在。
    fn read(&self) -> Option<LlmConfig>;
    /// 写入配置。覆盖任何现有内容。
    fn write(&self, config: &LlmConfig) -> Result<(), String>;
}

/// `GET /api/llm-status` 的响应体。
///
/// JSON 形状（R10.1 / R10.3）：
/// ```json
/// { "configured": true,  "provider": "deepseek" }
/// { "configured": false, "provider": null }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmStatusResponse {
    /// 是否已配置可用的 LLM Key。
    pub configured: bool,
    /// 已配置时为 Some(provider)，未配置时为 None（序列化为 `null`）。
    pub provider: Option<LlmProvider>,
    /// 配置的自定义 API Base URL（不包含敏感的 API Key）。
    pub base_url: Option<String>,
    /// 配置的自定义模型（不包含敏感信息）。
    pub model: Option<String>,
}

/// 由可选的 [`LlmConfig`] 计算 [`LlmStatusResponse`]。
///
/// 真值表（R10.2 / R10.3）：
/// - `None` ⇒ `{configured: false, provider: None}`
/// - `Some(cfg)` 且 `cfg.api_key` 为空 ⇒ `{configured: false, provider: None}`
/// - `Some(cfg)` 且 `cfg.api_key` 非空 ⇒
///   `{configured: true, provider: Some(cfg.provider)}`
///
/// 该函数是纯函数：不接触文件系统、网络或全局状态。所有 I/O 决策由调用方
/// （task 10.6 中的 handler）完成。
#[must_use]
pub fn status_from_config(input: Option<LlmConfig>) -> LlmStatusResponse {
    match input {
        Some(cfg) if cfg.is_configured() => LlmStatusResponse {
            configured: true,
            provider: Some(cfg.provider),
            base_url: cfg.base_url,
            model: cfg.model,
        },
        _ => LlmStatusResponse {
            configured: false,
            provider: None,
            base_url: None,
            model: None,
        },
    }
}

// ---------------------------------------------------------------------------
// HTTP handler (task 10.6)
// ---------------------------------------------------------------------------

/// `GET /api/llm-status` — 查询 LLM 配置状态。
///
/// 当 `AppState.llm_config_store` 为 None 时回退为 unconfigured，不报错。
/// 响应不含 `api_key`（Requirement 10.4）。
pub async fn get_llm_status(State(state): State<AppState>) -> Json<LlmStatusResponse> {
    let config = state
        .llm_config_store
        .as_ref()
        .and_then(|store| store.read());
    Json(status_from_config(config))
}

// ---------------------------------------------------------------------------
// POST /api/llm-config — test_and_save (task 10.7)
// ---------------------------------------------------------------------------

/// 连通性探测抽象层。
///
/// launcher 注入真实实现（向 LLM API 发一个轻量请求确认 key 有效）；
/// 测试注入 mock。
#[async_trait::async_trait]
pub trait LlmProbe: Send + Sync {
    /// 使用给定的 provider + `api_key` + `base_url` + model 发起一次连通性探测。
    /// Ok(()) → key 有效；Err(msg) → key 无效或网络不通。
    async fn probe(
        &self,
        provider: LlmProvider,
        api_key: &str,
        base_url: Option<&str>,
        model: Option<&str>,
    ) -> Result<(), String>;
}

/// `POST /api/llm-config` 的请求体。
#[derive(Debug, Clone, Deserialize)]
pub struct PostLlmConfigRequest {
    pub provider: LlmProvider,
    pub api_key: String,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

/// `POST /api/llm-config` 的 422 错误响应体。
#[derive(Debug, Clone, Serialize)]
pub struct LlmProbeFailedResponse {
    pub error_code: &'static str,
    pub message: String,
}

/// 核心业务逻辑：探测后决定是否落盘。
///
/// - `probe` Ok → 写入 `config_store` 并返回 `Ok(())`
/// - `probe` Err → 不写入并返回 `Err(message)`
///
/// 这是纯服务函数，不直接操作 HTTP。
pub async fn test_and_save(
    probe: &dyn LlmProbe,
    store: &dyn LlmConfigStore,
    provider: LlmProvider,
    api_key: &str,
    base_url: Option<&str>,
    model: Option<&str>,
) -> Result<(), String> {
    probe.probe(provider, api_key, base_url, model).await?;
    let config = LlmConfig {
        provider,
        api_key: api_key.to_owned(),
        base_url: base_url.map(String::from),
        model: model.map(String::from),
    };
    store.write(&config)?;
    Ok(())
}

/// `POST /api/llm-config` — 测试并保存 LLM 配置。
///
/// 成功 → 200（空 body）
/// 探测失败 → 422 + `LLM_PROBE_FAILED`
/// 缺少 config store 或 probe → 500
pub async fn post_llm_config(
    State(state): State<AppState>,
    Json(body): Json<PostLlmConfigRequest>,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let Some(store) = state.llm_config_store.as_ref() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error_code": "InternalError", "message": "LLM config store not configured"})),
        ).into_response();
    };

    let Some(probe) = state.llm_probe.as_ref() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error_code": "InternalError", "message": "LLM probe not configured"})),
        ).into_response();
    };

    match test_and_save(
        probe.as_ref(),
        store.as_ref(),
        body.provider,
        &body.api_key,
        body.base_url.as_deref(),
        body.model.as_deref(),
    )
    .await
    {
        Ok(()) => StatusCode::OK.into_response(),
        Err(msg) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(LlmProbeFailedResponse {
                error_code: "LLM_PROBE_FAILED",
                message: msg,
            }),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_input_yields_unconfigured() {
        let resp = status_from_config(None);
        assert!(!resp.configured);
        assert_eq!(resp.provider, None);
    }

    #[test]
    fn empty_api_key_yields_unconfigured() {
        let cfg = LlmConfig {
            provider: LlmProvider::DeepSeek,
            api_key: String::new(),
            base_url: None,
            model: None,
        };
        let resp = status_from_config(Some(cfg));
        assert!(!resp.configured);
        assert_eq!(resp.provider, None);
    }

    #[test]
    fn nonempty_api_key_yields_configured_with_provider() {
        let cfg = LlmConfig {
            provider: LlmProvider::OpenAi,
            api_key: "sk-test-1234".into(),
            base_url: None,
            model: None,
        };
        let resp = status_from_config(Some(cfg));
        assert!(resp.configured);
        assert_eq!(resp.provider, Some(LlmProvider::OpenAi));
    }

    #[test]
    fn configured_response_serializes_to_lowercase_provider() {
        let cfg = LlmConfig {
            provider: LlmProvider::DeepSeek,
            api_key: "sk-abc".into(),
            base_url: None,
            model: None,
        };
        let resp = status_from_config(Some(cfg));
        let json = serde_json::to_string(&resp).expect("serialize");
        assert_eq!(
            json,
            r#"{"configured":true,"provider":"deepseek","base_url":null,"model":null}"#
        );
    }

    #[test]
    fn unconfigured_response_serializes_provider_as_null() {
        let resp = status_from_config(None);
        let json = serde_json::to_string(&resp).expect("serialize");
        assert_eq!(
            json,
            r#"{"configured":false,"provider":null,"base_url":null,"model":null}"#
        );
    }

    #[test]
    fn openai_provider_serializes_to_openai_lowercase() {
        let cfg = LlmConfig {
            provider: LlmProvider::OpenAi,
            api_key: "sk-xyz".into(),
            base_url: Some("https://custom.url".into()),
            model: Some("gpt-4o-custom".into()),
        };
        let resp = status_from_config(Some(cfg));
        let json = serde_json::to_string(&resp).expect("serialize");
        assert_eq!(
            json,
            r#"{"configured":true,"provider":"openai","base_url":"https://custom.url","model":"gpt-4o-custom"}"#
        );
    }
}
