mod commands;
mod context;
mod discovery;
mod dispatch;
mod display;
mod render;
mod repl;
mod session;
mod tools;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use api::{detect_provider_kind, resolve_model_alias, InputMessage};
use serde_json::Value;

use crate::cli::{AuthSetArgs, ChatArgs, Cli, ConfigModelArgs};
use crate::config::{
    current_stats_code_profile, estimate_session_cost_usd, extract_response_text, handle_auth_set,
    handle_config_add_model, handle_config_default_model, handle_config_remove_model,
    handle_config_show, load_auth_store, load_stats_code_settings, parse_auth_provider_name,
    prepare_ai_provider, resolve_requested_model, save_auth_store, save_stats_code_settings,
    stats_code_auth_path, stats_code_env_path, stats_code_profile_path, stats_code_settings_path,
    ChatUsageTotals, ModelPricing,
};
use crate::helpers::{stringify_error, unix_timestamp_nanos};
use crate::render::{render_auth_set_text, render_config_text};

// Re-exports from sub-modules used by commands.rs (via `use super::*`)
#[cfg(test)]
use self::commands::handle_chat_command;
use self::context::{collect_project_context, format_project_context_summary};
use self::discovery::{
    append_memory_note, discover_slash_command_templates, primary_memory_file_path,
    render_custom_command_prompt,
};
use self::dispatch::{handle_shell_bang, run_chat_turn, run_one_shot_prompt, run_process_capture};
use self::display::{format_token_count, truncate_for_display};
use self::render::{
    render_builtin_slash_help, render_mcp_overview, render_plugin_overview, render_skill_overview,
    render_status_report,
};
use self::repl::{append_chat_exchange, record_session_usage};
use self::session::{default_chat_session_path, load_chat_session, save_chat_session};

// ---------------------------------------------------------------------------
// Struct / enum / const definitions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct ChatSessionState {
    pub(crate) model: String,
    pub(crate) system: Option<String>,
    pub(crate) max_tokens: Option<u32>,
    pub(crate) use_tools: bool,
    pub(crate) fast_mode: bool,
    pub(crate) vim_mode: bool,
    pub(crate) artifacts_dir: Option<PathBuf>,
    pub(crate) session_path: PathBuf,
    pub(crate) session_loaded: bool,
    pub(crate) project_context: ChatProjectContext,
    pub(crate) usage: ChatUsageTotals,
    pub(crate) last_request_id: Option<String>,
    pub(crate) messages: Vec<InputMessage>,
}

#[derive(Debug, Clone)]
pub(crate) struct ChatTurnOutput {
    pub(crate) response_text: String,
    pub(crate) input_tokens: u32,
    pub(crate) output_tokens: u32,
    pub(crate) tool_calls: usize,
    pub(crate) request_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingToolUse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) input: Value,
}

pub(crate) struct BuiltinSlashCommand {
    pub(crate) name: &'static str,
    pub(crate) args: &'static str,
    pub(crate) description_zh: &'static str,
}

pub(crate) const BUILTIN_SLASH_COMMANDS: &[BuiltinSlashCommand] = &[
    BuiltinSlashCommand {
        name: "help",
        args: "",
        description_zh: "显示帮助信息",
    },
    BuiltinSlashCommand {
        name: "clear",
        args: "",
        description_zh: "清除对话历史",
    },
    BuiltinSlashCommand {
        name: "compact",
        args: "",
        description_zh: "压缩对话上下文（节省 token）",
    },
    BuiltinSlashCommand {
        name: "cost",
        args: "",
        description_zh: "显示当前会话的 token 用量和费用",
    },
    BuiltinSlashCommand {
        name: "status",
        args: "",
        description_zh: "显示当前配置状态",
    },
    BuiltinSlashCommand {
        name: "model",
        args: "",
        description_zh: "切换模型（如 sonnet/opus/haiku）",
    },
    BuiltinSlashCommand {
        name: "fast",
        args: "",
        description_zh: "切换 Fast 模式（更快输出）",
    },
    BuiltinSlashCommand {
        name: "memory",
        args: "",
        description_zh: "查看/编辑记忆文件",
    },
    BuiltinSlashCommand {
        name: "config",
        args: "",
        description_zh: "查看或修改配置",
    },
    BuiltinSlashCommand {
        name: "review",
        args: "",
        description_zh: "代码审查",
    },
    BuiltinSlashCommand {
        name: "pr_comments",
        args: "",
        description_zh: "查看 PR 评论",
    },
    BuiltinSlashCommand {
        name: "init",
        args: "",
        description_zh: "初始化项目（生成 STATS.md）",
    },
    BuiltinSlashCommand {
        name: "login",
        args: "",
        description_zh: "登录账号/保存凭据",
    },
    BuiltinSlashCommand {
        name: "logout",
        args: "",
        description_zh: "退出登录/移除凭据",
    },
    BuiltinSlashCommand {
        name: "bug",
        args: "",
        description_zh: "报告 Stats Code 的 bug",
    },
    BuiltinSlashCommand {
        name: "release-notes",
        args: "",
        description_zh: "查看更新日志",
    },
    BuiltinSlashCommand {
        name: "vim",
        args: "",
        description_zh: "切换 Vim 输入模式",
    },
    BuiltinSlashCommand {
        name: "terminal-setup",
        args: "",
        description_zh: "配置终端（shift+enter 换行等）",
    },
    BuiltinSlashCommand {
        name: "plugin",
        args: "",
        description_zh: "查看插件",
    },
    BuiltinSlashCommand {
        name: "skill",
        args: "",
        description_zh: "查看技能",
    },
    BuiltinSlashCommand {
        name: "mcp",
        args: "",
        description_zh: "查看 MCP 配置",
    },
];

#[derive(Debug, Clone)]
pub(crate) struct ChatProjectContext {
    pub(crate) cwd: PathBuf,
    pub(crate) files: Vec<ChatContextFile>,
}

#[derive(Debug, Clone)]
pub(crate) struct ChatContextFile {
    pub(crate) label: String,
    pub(crate) content: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SlashCommandTemplate {
    pub(crate) name: String,
    pub(crate) _description: Option<String>,
    pub(crate) body: String,
    pub(crate) path: PathBuf,
    pub(crate) source: String,
}

pub(crate) enum ChatLoopControl {
    Continue,
    Exit,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub(crate) fn run_chat_repl(cli: &Cli, chat_args: &ChatArgs) -> Result<(), String> {
    if cli.json {
        return Err(
            "Interactive chat mode does not support `--json`. Use `stats-code ai ask --json ...` for one-shot structured output.".to_string(),
        );
    }

    let cwd = env::current_dir().map_err(stringify_error)?;
    let session_path = cli
        .session
        .clone()
        .unwrap_or_else(|| default_chat_session_path(&cwd));
    let project_context = collect_project_context(&cwd)?;
    let mut state = ChatSessionState {
        model: resolve_requested_model(&cli.model),
        system: cli.system.clone(),
        max_tokens: cli.max_tokens,
        use_tools: !chat_args.no_tools,
        fast_mode: false,
        vim_mode: false,
        artifacts_dir: cli.artifacts_dir.clone(),
        session_path,
        session_loaded: false,
        project_context,
        usage: ChatUsageTotals::default(),
        last_request_id: None,
        messages: Vec::new(),
    };

    if !chat_args.new_session {
        if let Some(saved) = load_chat_session(&state.session_path)? {
            state.messages = saved.messages;
            if cli.model == "gpt" {
                state.model = saved.model;
            }
            if state.system.is_none() {
                state.system = saved.system;
            }
            if state.max_tokens.is_none() {
                state.max_tokens = saved.max_tokens;
            }
            if !chat_args.no_tools {
                state.use_tools = saved.use_tools;
            }
            state.fast_mode = saved.fast_mode;
            state.vim_mode = saved.vim_mode;
            state.usage = ChatUsageTotals {
                input_tokens: saved.input_tokens_total,
                output_tokens: saved.output_tokens_total,
                tool_calls: saved.tool_calls_total,
                turns: saved.turns_total,
            };
            state.last_request_id = saved.last_request_id;
            state.session_loaded = true;
        }
    }

    repl::run_chat_repl(state)
}

#[cfg(test)]
#[path = "tests_mod.rs"]
mod tests;
