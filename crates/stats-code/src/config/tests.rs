use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use super::*;
use crate::cli::Cli;
use crate::cli::{
    AuthCommand, AuthDoctorArgs, AuthProvider, AuthSetArgs, Command, ConfigCommand,
    ConfigModelArgs,
};
use crate::handlers::dispatch;

fn temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("epistat-{label}-{nanos}"))
}

fn test_cli(command: Command) -> Cli {
    Cli {
        json: false,
        artifacts_dir: None,
        session: None,
        model: "gpt".to_string(),
        system: None,
        max_tokens: None,
        engine: crate::bridge::Engine::Rust,
        alpha: 0.05,
        na_strategy: crate::cli::NaStrategy::Drop,
        command: Some(command),
    }
}

struct EnvVarGuard {
    key: &'static str,
    original: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: Option<&str>) -> Self {
        let original = std::env::var(key).ok();
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

fn env_test_guard() -> MutexGuard<'static, ()> {
    static ENV_TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    ENV_TEST_MUTEX
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env test mutex poisoned")
}

#[test]
fn auth_store_round_trip_persists_saved_credentials() {
    let root = temp_dir("auth-store");
    fs::create_dir_all(&root).expect("create root");
    let auth_path = root.join("auth.json");
    let mut store = StoredAuthStore::default();
    store.providers.insert(
        "openai".to_string(),
        StoredProviderCredential {
            api_key: "sk-test".to_string(),
            base_url: Some("https://example.invalid/v1".to_string()),
            updated_at_unix_nanos: 42,
        },
    );

    save_auth_store(&auth_path, &store).expect("save auth store");
    let loaded = load_auth_store(&auth_path).expect("load auth store");
    let openai = loaded.providers.get("openai").expect("openai credential");
    assert_eq!(openai.api_key, "sk-test");
    assert_eq!(
        openai.base_url.as_deref(),
        Some("https://example.invalid/v1")
    );

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn auth_commands_save_and_report_saved_credentials() {
    let _env_guard = env_test_guard();
    let root = temp_dir("auth-cli");
    fs::create_dir_all(&root).expect("create root");
    let _config_guard = EnvVarGuard::set(
        "STATS_CODE_CONFIG_DIR",
        Some(root.to_str().expect("utf8 path")),
    );
    let _openai_key_guard = EnvVarGuard::set("OPENAI_API_KEY", None);
    let _openai_base_guard = EnvVarGuard::set("OPENAI_BASE_URL", None);

    let set_cli = test_cli(Command::Auth {
        command: AuthCommand::Set(AuthSetArgs {
            provider: AuthProvider::Openai,
            api_key: "sk-test".to_string(),
            base_url: Some("https://example.invalid/v1".to_string()),
        }),
    });
    let rendered = dispatch(&set_cli).expect("auth set should succeed");
    assert!(rendered.contains("Auth Set"));

    let doctor_cli = test_cli(Command::Auth {
        command: AuthCommand::Doctor(AuthDoctorArgs {
            provider: Some(AuthProvider::Openai),
        }),
    });
    let rendered = dispatch(&doctor_cli).expect("auth doctor should succeed");
    assert!(rendered.contains("source=saved_config"));
    assert!(rendered.contains("configured_base_url=https://example.invalid/v1"));
    assert!(root.join("auth.json").is_file());

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn config_commands_persist_default_and_saved_models() {
    let _env_guard = env_test_guard();
    let root = temp_dir("config-cli");
    fs::create_dir_all(&root).expect("create root");
    let _config_guard = EnvVarGuard::set(
        "STATS_CODE_CONFIG_DIR",
        Some(root.to_str().expect("utf8 path")),
    );

    let add_cli = test_cli(Command::Config {
        command: ConfigCommand::AddModel(ConfigModelArgs {
            model: "gemini-2.5-pro".to_string(),
        }),
    });
    let add_rendered = dispatch(&add_cli).expect("config add-model should succeed");
    assert!(add_rendered.contains("Saved models"));
    assert!(add_rendered.contains("gemini-2.5-pro"));

    let default_cli = test_cli(Command::Config {
        command: ConfigCommand::DefaultModel(ConfigModelArgs {
            model: "gemini-2.5-pro".to_string(),
        }),
    });
    let default_rendered = dispatch(&default_cli).expect("config default-model should succeed");
    assert!(default_rendered.contains("Default model"));
    assert!(default_rendered.contains("gemini-2.5-pro"));

    let show_cli = test_cli(Command::Config {
        command: ConfigCommand::Show,
    });
    let show_rendered = dispatch(&show_cli).expect("config show should succeed");
    assert!(show_rendered.contains("Loaded Stats Code settings"));
    assert!(show_rendered.contains("gemini-2.5-pro"));

    let settings =
        load_stats_code_settings(&stats_code_settings_path()).expect("load settings");
    assert_eq!(settings.default_model.as_deref(), Some("gemini-2.5-pro"));
    assert_eq!(resolve_requested_model("gpt"), "gemini-2.5-pro");

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn profile_files_can_persist_opencode_style_defaults() {
    let _env_guard = env_test_guard();
    let root = temp_dir("profile-cli");
    fs::create_dir_all(&root).expect("create root");
    let _config_guard = EnvVarGuard::set(
        "STATS_CODE_CONFIG_DIR",
        Some(root.to_str().expect("utf8 path")),
    );

    let mut profile = StatsCodeProfile {
        model_provider: Some("OpenAI".to_string()),
        model: Some("gpt-5.4".to_string()),
        review_model: Some("gpt-5.4".to_string()),
        model_reasoning_effort: Some("xhigh".to_string()),
        disable_response_storage: Some(true),
        network_access: Some("enabled".to_string()),
        windows_wsl_setup_acknowledged: Some(true),
        model_context_window: Some(1_000_000),
        model_auto_compact_token_limit: Some(900_000),
        model_providers: BTreeMap::new(),
    };
    profile.model_providers.insert(
        "OpenAI".to_string(),
        StatsCodeProviderProfile {
            name: Some("OpenAI".to_string()),
            base_url: Some("https://gmn.chuangzuoli.com".to_string()),
            wire_api: Some("responses".to_string()),
            requires_openai_auth: Some(true),
        },
    );
    save_stats_code_profile(&stats_code_profile_path(), &profile).expect("save profile");
    save_stats_code_env(
        &stats_code_env_path(),
        &StatsCodeProfileEnv {
            entries: BTreeMap::from([(
                "OPENAI_API_KEY".to_string(),
                "sk-test-profile".to_string(),
            )]),
        },
    )
    .expect("save profile env");

    let loaded_profile =
        load_stats_code_profile(&stats_code_profile_path()).expect("load profile");
    assert_eq!(loaded_profile.model.as_deref(), Some("gpt-5.4"));
    assert_eq!(resolve_requested_model("gpt"), "gpt-5.4");

    let rendered = dispatch(&test_cli(Command::Auth {
        command: AuthCommand::Doctor(AuthDoctorArgs {
            provider: Some(AuthProvider::Openai),
        }),
    }))
    .expect("auth doctor should succeed");
    assert!(rendered.contains("source=profile_config"));
    assert!(rendered.contains("configured_base_url=https://gmn.chuangzuoli.com/v1"));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn prepare_ai_provider_uses_saved_credentials_without_mutating_env() {
    let _env_guard = env_test_guard();
    let root = temp_dir("prepare-ai-provider-saved");
    fs::create_dir_all(&root).expect("create root");
    let _config_guard = EnvVarGuard::set(
        "STATS_CODE_CONFIG_DIR",
        Some(root.to_str().expect("utf8 path")),
    );
    let _openai_key_guard = EnvVarGuard::set("OPENAI_API_KEY", None);
    let _openai_base_guard = EnvVarGuard::set("OPENAI_BASE_URL", None);

    let mut store = StoredAuthStore::default();
    store.providers.insert(
        "openai".to_string(),
        StoredProviderCredential {
            api_key: "sk-test-saved".to_string(),
            base_url: Some("https://example.invalid/v1".to_string()),
            updated_at_unix_nanos: 42,
        },
    );
    save_auth_store(&stats_code_auth_path(), &store).expect("save auth store");

    let prepared =
        prepare_ai_provider(api::ProviderKind::OpenAi, "gpt-5.4").expect("prepare provider");
    assert_eq!(prepared.credential_source, "saved_config");
    assert_eq!(prepared.client.provider_kind(), api::ProviderKind::OpenAi);
    assert_eq!(std::env::var("OPENAI_API_KEY").ok(), None);
    assert_eq!(std::env::var("OPENAI_BASE_URL").ok(), None);

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn estimates_cost_from_pricing_table() {
    let mut pricing = BTreeMap::new();
    pricing.insert(
        "gpt-5.4".to_string(),
        ModelPricing {
            input_per_million_usd: 5.0,
            output_per_million_usd: 15.0,
        },
    );
    let usage = ChatUsageTotals {
        input_tokens: 100_000,
        output_tokens: 20_000,
        tool_calls: 0,
        turns: 1,
    };

    let cost = estimate_session_cost_usd(&pricing, "gpt", &usage).expect("cost estimate");
    assert!((cost - 0.8).abs() < 1e-9);
}
