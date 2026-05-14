use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use api::InputMessage;

use super::*;
use crate::config::ChatUsageTotals;

fn temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("epistat-{label}-{nanos}"))
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
fn chat_session_round_trip_persists_messages_and_settings() {
    let root = temp_dir("chat-session");
    fs::create_dir_all(&root).expect("create root");
    let session_path = root.join("saved-session.json");
    let state = ChatSessionState {
        model: "gemini".to_string(),
        system: Some("be concise".to_string()),
        max_tokens: Some(512),
        use_tools: false,
        fast_mode: true,
        vim_mode: false,
        artifacts_dir: None,
        session_path: session_path.clone(),
        session_loaded: false,
        project_context: ChatProjectContext {
            cwd: root.clone(),
            files: Vec::new(),
        },
        usage: ChatUsageTotals {
            input_tokens: 120,
            output_tokens: 45,
            tool_calls: 2,
            turns: 3,
        },
        last_request_id: Some("req_123".to_string()),
        messages: vec![
            InputMessage::user_text("hello"),
            InputMessage {
                role: "assistant".to_string(),
                content: vec![api::InputContentBlock::Text {
                    text: "world".to_string(),
                }],
            },
        ],
    };

    save_chat_session(&state).expect("save chat session");
    let saved = load_chat_session(&session_path)
        .expect("load chat session")
        .expect("session exists");
    assert_eq!(saved.model, "gemini");
    assert_eq!(saved.system.as_deref(), Some("be concise"));
    assert_eq!(saved.max_tokens, Some(512));
    assert!(!saved.use_tools);
    assert!(saved.fast_mode);
    assert!(!saved.vim_mode);
    assert_eq!(saved.input_tokens_total, 120);
    assert_eq!(saved.output_tokens_total, 45);
    assert_eq!(saved.tool_calls_total, 2);
    assert_eq!(saved.turns_total, 3);
    assert_eq!(saved.last_request_id.as_deref(), Some("req_123"));
    assert_eq!(saved.messages.len(), 2);

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn project_context_loads_priority_files_from_workspace() {
    let root = temp_dir("project-context");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("AGENTS.md"), "Project rules").expect("write agents");
    fs::write(root.join("README.md"), "Readme body").expect("write readme");
    fs::write(root.join("analysis.yaml"), "study:\n  title: Demo\n").expect("write analysis");

    let context = collect_project_context(&root).expect("collect context");
    assert_eq!(context.cwd, root);
    assert!(context.files.iter().any(|file| file.label == "AGENTS.md"));
    assert!(context.files.iter().any(|file| file.label == "README.md"));
    assert!(context
        .files
        .iter()
        .any(|file| file.label == "analysis.yaml"));

    fs::remove_dir_all(context.cwd).expect("cleanup");
}

#[test]
fn default_chat_session_path_is_stable_for_workspace() {
    let root = PathBuf::from(r"C:\workspace\stats-project");
    let left = default_chat_session_path(&root);
    let right = default_chat_session_path(&root);
    assert_eq!(left, right);
    assert!(left.to_string_lossy().contains("stats-project"));
    assert_eq!(
        left.extension().and_then(|value| value.to_str()),
        Some("json")
    );
}

#[test]
fn discovers_project_user_and_plugin_slash_commands() {
    let _env_guard = env_test_guard();
    let project_root = temp_dir("slash-project");
    let user_home = temp_dir("slash-home");
    fs::create_dir_all(project_root.join(".stats-code").join("commands"))
        .expect("project commands");
    fs::create_dir_all(
        project_root
            .join(".stats-code")
            .join("plugins")
            .join("demo")
            .join(".stats-code-plugin")
            .join("commands"),
    )
    .expect("plugin commands");
    fs::create_dir_all(user_home.join(".stats-code").join("commands")).expect("user commands");
    fs::write(
        project_root
            .join(".stats-code")
            .join("commands")
            .join("project.md"),
        "---\ndescription: Project command\n---\nproject body",
    )
    .expect("write project command");
    fs::write(
        project_root
            .join(".stats-code")
            .join("plugins")
            .join("demo")
            .join(".stats-code-plugin")
            .join("plugin.json"),
        r#"{"name":"demo"}"#,
    )
    .expect("write plugin manifest");
    fs::write(
        project_root
            .join(".stats-code")
            .join("plugins")
            .join("demo")
            .join(".stats-code-plugin")
            .join("commands")
            .join("plugin-cmd.md"),
        "---\ndescription: Plugin command\n---\nplugin body",
    )
    .expect("write plugin command");
    fs::write(
        user_home.join(".stats-code").join("commands").join("user.md"),
        "---\ndescription: User command\n---\nuser body",
    )
    .expect("write user command");

    let _home_guard = EnvVarGuard::set("HOME", Some(user_home.to_str().expect("utf8")));
    let _userprofile_guard =
        EnvVarGuard::set("USERPROFILE", Some(user_home.to_str().expect("utf8")));

    let commands =
        discover_slash_command_templates(&project_root).expect("discover slash commands");
    let names = commands
        .iter()
        .map(|command| command.name.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"project"));
    assert!(names.contains(&"user"));
    assert!(names.contains(&"plugin-cmd"));
    assert!(commands
        .iter()
        .any(|command| command.source.contains("project .stats-code/commands")));
    assert!(commands
        .iter()
        .any(|command| command.source.contains("user ~/.stats-code/commands")));
    assert!(commands
        .iter()
        .any(|command| command.source.contains("project-plugin:demo")));

    fs::remove_dir_all(project_root).expect("cleanup project");
    fs::remove_dir_all(user_home).expect("cleanup user home");
}

#[test]
fn bare_slash_is_shortcut_for_help() {
    let root = temp_dir("slash-help");
    fs::create_dir_all(&root).expect("create root");
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let mut output = Vec::new();
    let mut state = ChatSessionState {
        model: "gpt".to_string(),
        system: None,
        max_tokens: None,
        use_tools: true,
        fast_mode: false,
        vim_mode: false,
        artifacts_dir: None,
        session_path: root.join("session.json"),
        session_loaded: false,
        project_context: ChatProjectContext {
            cwd: root.clone(),
            files: Vec::new(),
        },
        usage: ChatUsageTotals::default(),
        last_request_id: None,
        messages: Vec::new(),
    };

    let result = handle_chat_command("/", &mut state, &mut output, &runtime)
        .expect("slash help should succeed");
    assert!(matches!(result, ChatLoopControl::Continue));
    let rendered = String::from_utf8(output).expect("utf8 output");
    assert!(rendered.contains("Slash commands"));
    assert!(rendered.contains("/help"));

    fs::remove_dir_all(root).expect("cleanup");
}
