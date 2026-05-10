mod commands;
mod context;
mod discovery;
mod display;
mod session;
mod tools;

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use api::{
    detect_provider_kind, max_tokens_for_model, resolve_model_alias, InputContentBlock,
    InputMessage, MessageRequest, ToolChoice,
};
use colored::Colorize;
use serde_json::Value;

use crate::cli::{AuthSetArgs, ChatArgs, Cli, ConfigModelArgs};
use crate::config::{
    current_stats_code_profile, estimate_session_cost_usd, extract_response_text, handle_auth_set,
    handle_config_add_model, handle_config_default_model, handle_config_remove_model,
    handle_config_show, home_dir, load_auth_store, load_stats_code_settings,
    parse_auth_provider_name, prepare_ai_provider, resolve_requested_model, save_auth_store,
    save_stats_code_settings, stats_code_auth_path, stats_code_env_path, stats_code_profile_path,
    stats_code_settings_path, ChatUsageTotals, ModelPricing,
};
use crate::helpers::{stringify_error, unix_timestamp_nanos};
use crate::input::{LineEditor, ReadOutcome};
use crate::render::{render_auth_set_text, render_config_text};
use crate::ui::{ChatEntryKind, ChatUi};

// Re-exports from sub-modules
use self::commands::handle_chat_command;
use self::context::{
    build_chat_system_prompt, collect_project_context, format_project_context_summary,
};
use self::discovery::{
    append_memory_note, collect_plugin_manifests, discover_slash_command_templates,
    nearest_project_claude_dir, primary_memory_file_path, render_custom_command_prompt,
    slash_command_completion_candidates,
};
use self::display::{
    build_chat_ui_status, format_token_count, print_ui_output, truncate_for_display,
};
use self::session::{default_chat_session_path, load_chat_session, save_chat_session};
use self::tools::{
    assistant_message_from_response, chat_tool_definitions, collect_pending_tool_uses,
    execute_chat_tool, summarize_tool_input_short,
};

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
    response_text: String,
    input_tokens: u32,
    output_tokens: u32,
    tool_calls: usize,
    request_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingToolUse {
    id: String,
    name: String,
    input: Value,
}

pub(crate) struct BuiltinSlashCommand {
    name: &'static str,
    args: &'static str,
    description_zh: &'static str,
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
        description_zh: "初始化项目（生成 CLAUDE.md）",
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

    run_chat_repl_claude_style(state)
}

fn run_chat_repl_claude_style(mut state: ChatSessionState) -> Result<(), String> {
    save_chat_session(&state)?;

    let mut stdout = io::stdout();
    let runtime = tokio::runtime::Runtime::new().map_err(stringify_error)?;
    let ui = ChatUi::new();
    let mut editor = LineEditor::new(
        "> ",
        slash_command_completion_candidates(&state.project_context.cwd),
    );

    ui.print_welcome(&mut stdout, &build_chat_ui_status(&state))
        .map_err(stringify_error)?;

    if state.session_loaded {
        ui.print_turn(
            &mut stdout,
            ChatEntryKind::System,
            &format!(
                "Resumed session from {} with {} stored messages.",
                state.session_path.display(),
                state.messages.len()
            ),
        )
        .map_err(stringify_error)?;
    } else {
        ui.print_turn(
            &mut stdout,
            ChatEntryKind::System,
            &format!(
                "Workspace {} ready. Type `/` for commands or `!` for shell.",
                state.project_context.cwd.display()
            ),
        )
        .map_err(stringify_error)?;
    }

    loop {
        ui.print_status_bar(&mut stdout, &build_chat_ui_status(&state), None)
            .map_err(stringify_error)?;

        let input = match editor.read_line().map_err(stringify_error)? {
            ReadOutcome::Submit(line) => line,
            ReadOutcome::Cancel => continue,
            ReadOutcome::Exit => break,
        };
        // Close input box bottom border
        ui.print_input_bottom(&mut stdout)
            .map_err(stringify_error)?;

        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }
        editor.push_history(trimmed.to_string());

        if let Some(shell_command) = trimmed.strip_prefix('!').map(str::trim) {
            if shell_command.is_empty() {
                ui.print_turn(
                    &mut stdout,
                    ChatEntryKind::Error,
                    "Missing shell command after `!`.",
                )
                .map_err(stringify_error)?;
                continue;
            }
            ui.print_turn(
                &mut stdout,
                ChatEntryKind::User,
                &format!("! {shell_command}"),
            )
            .map_err(stringify_error)?;
            ui.print_status_bar(
                &mut stdout,
                &build_chat_ui_status(&state),
                Some("Running shell command..."),
            )
            .map_err(stringify_error)?;
            let mut output = Vec::new();
            match handle_shell_bang(shell_command, &mut state, &mut output) {
                Ok(ChatLoopControl::Exit) => break,
                Ok(ChatLoopControl::Continue) => {
                    print_ui_output(&ui, &mut stdout, ChatEntryKind::Tool, &output);
                }
                Err(err) => {
                    print_ui_output(&ui, &mut stdout, ChatEntryKind::Tool, &output);
                    ui.print_turn(&mut stdout, ChatEntryKind::Error, &err)
                        .map_err(stringify_error)?;
                }
            }
            continue;
        }

        if trimmed.starts_with('/') {
            ui.print_turn(&mut stdout, ChatEntryKind::User, trimmed)
                .map_err(stringify_error)?;
            ui.print_status_bar(
                &mut stdout,
                &build_chat_ui_status(&state),
                Some("Running slash command..."),
            )
            .map_err(stringify_error)?;
            let mut output = Vec::new();
            match handle_chat_command(trimmed, &mut state, &mut output, &runtime) {
                Ok(ChatLoopControl::Exit) => break,
                Ok(ChatLoopControl::Continue) => {
                    if trimmed == "/clear" {
                        crossterm::execute!(
                            &mut stdout,
                            crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
                        )
                        .map_err(stringify_error)?;
                    }
                    print_ui_output(&ui, &mut stdout, ChatEntryKind::System, &output);
                }
                Err(err) => {
                    print_ui_output(&ui, &mut stdout, ChatEntryKind::System, &output);
                    ui.print_turn(&mut stdout, ChatEntryKind::Error, &err)
                        .map_err(stringify_error)?;
                }
            }
            continue;
        }

        ui.print_turn(&mut stdout, ChatEntryKind::User, trimmed)
            .map_err(stringify_error)?;
        ui.print_status_bar(
            &mut stdout,
            &build_chat_ui_status(&state),
            Some("Waiting for model response..."),
        )
        .map_err(stringify_error)?;

        let mut tool_output = Vec::new();
        let turn = match run_chat_turn(&mut state, trimmed, &mut tool_output, &runtime) {
            Ok(t) => t,
            Err(err) => {
                print_ui_output(&ui, &mut stdout, ChatEntryKind::Tool, &tool_output);
                ui.print_turn(
                    &mut stdout,
                    ChatEntryKind::Error,
                    &format!(
                        "{err}\nCheck your API key with `stats-code auth status`, or type /help."
                    ),
                )
                .map_err(stringify_error)?;
                continue;
            }
        };
        print_ui_output(&ui, &mut stdout, ChatEntryKind::Tool, &tool_output);
        ui.print_turn(
            &mut stdout,
            ChatEntryKind::Assistant,
            turn.response_text.trim(),
        )
        .map_err(stringify_error)?;
    }

    Ok(())
}

fn record_session_usage(
    state: &mut ChatSessionState,
    input_tokens: u32,
    output_tokens: u32,
    tool_calls: usize,
    request_id: Option<String>,
) {
    state.usage.input_tokens = state
        .usage
        .input_tokens
        .saturating_add(u64::from(input_tokens));
    state.usage.output_tokens = state
        .usage
        .output_tokens
        .saturating_add(u64::from(output_tokens));
    state.usage.tool_calls = state.usage.tool_calls.saturating_add(tool_calls as u64);
    state.usage.turns = state.usage.turns.saturating_add(1);
    state.last_request_id = request_id;
}

fn run_process_capture(
    program: &str,
    args: &[&str],
    cwd: &Path,
) -> Result<(String, String, i32), String> {
    let output = ProcessCommand::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|error| format!("Failed to run `{program}`: {error}"))?;
    Ok((
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    ))
}

fn run_shell_capture(command: &str, cwd: &Path) -> Result<(String, String, i32), String> {
    if cfg!(windows) {
        run_process_capture(
            "powershell",
            &["-NoLogo", "-NoProfile", "-Command", command],
            cwd,
        )
    } else {
        run_process_capture("sh", &["-lc", command], cwd)
    }
}

fn render_status_report(state: &ChatSessionState) -> Result<String, String> {
    let settings = load_stats_code_settings(&stats_code_settings_path())?;
    let resolved_model = resolve_model_alias(&state.model);
    let mut out = String::new();
    let _ = writeln!(out, "Status");
    let _ = writeln!(out, "  Session path      {}", state.session_path.display());
    let _ = writeln!(
        out,
        "  Project cwd       {}",
        state.project_context.cwd.display()
    );
    let _ = writeln!(
        out,
        "  Model             {} -> {}",
        state.model, resolved_model
    );
    let _ = writeln!(
        out,
        "  Modes             tools={} fast={} vim={}",
        if state.use_tools { "on" } else { "off" },
        if state.fast_mode { "on" } else { "off" },
        if state.vim_mode { "on" } else { "off" }
    );
    let _ = writeln!(
        out,
        "  Context files     {}",
        state.project_context.files.len()
    );
    let _ = writeln!(
        out,
        "  Session usage     in={} out={} turns={} tools={}",
        format_token_count(state.usage.input_tokens),
        format_token_count(state.usage.output_tokens),
        state.usage.turns,
        state.usage.tool_calls
    );
    let _ = writeln!(
        out,
        "  Last request id   {}",
        state.last_request_id.as_deref().unwrap_or("<none>")
    );
    let _ = writeln!(
        out,
        "  Saved models      {}",
        if settings.saved_models.is_empty() {
            "<none>".to_string()
        } else {
            settings.saved_models.join(", ")
        }
    );
    let _ = writeln!(
        out,
        "  Default model     {}",
        settings.default_model.as_deref().unwrap_or("<none>")
    );
    let _ = writeln!(out, "  Pricing entries   {}", settings.pricing.len());
    Ok(out)
}

fn append_chat_exchange(
    state: &mut ChatSessionState,
    user_text: impl Into<String>,
    assistant_text: impl Into<String>,
) -> Result<(), String> {
    state
        .messages
        .push(InputMessage::user_text(user_text.into()));
    let assistant_text = assistant_text.into();
    if !assistant_text.trim().is_empty() {
        state.messages.push(InputMessage {
            role: "assistant".to_string(),
            content: vec![InputContentBlock::Text {
                text: assistant_text,
            }],
        });
    }
    save_chat_session(state)
}

fn render_builtin_slash_help() -> String {
    let mut out = String::new();
    let _ = writeln!(out, "  Slash commands:");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "┌──────────────────┬──────────────────────────────────┐"
    );
    let _ = writeln!(
        out,
        "│   命令             │            说明                │"
    );
    let _ = writeln!(
        out,
        "├──────────────────┼──────────────────────────────────┤"
    );
    for command in BUILTIN_SLASH_COMMANDS {
        let label = if command.args.is_empty() {
            format!("/{}", command.name)
        } else {
            format!("/{} {}", command.name, command.args)
        };
        let _ = writeln!(out, "│{:<15}   │{:<32}  │", label, command.description_zh);
        let _ = writeln!(
            out,
            "├──────────────────┼──────────────────────────────────┤"
        );
    }
    let _ = writeln!(
        out,
        "提示：输入 `/` 会弹出这组内置命令，不再自动混入自定义 slash command。"
    );
    out
}

fn render_plugin_overview(cwd: &Path) -> Result<String, String> {
    let mut manifests = Vec::new();
    if let Some(home) = home_dir() {
        collect_plugin_manifests(&home.join(".claude").join("plugins"), &mut manifests)?;
    }
    if let Some(project_claude_dir) = nearest_project_claude_dir(cwd) {
        collect_plugin_manifests(&project_claude_dir.join("plugins"), &mut manifests)?;
    }

    manifests.sort();
    manifests.dedup();

    let mut rows = Vec::new();
    for manifest_path in manifests {
        let text = fs::read_to_string(&manifest_path).map_err(stringify_error)?;
        let json = serde_json::from_str::<Value>(&text).map_err(stringify_error)?;
        let name = json.get("name").and_then(Value::as_str).unwrap_or("plugin");
        let version = json
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>");
        let commands = json
            .get("commands")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let skills = json
            .get("skills")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        rows.push(format!(
            "- {} v{}  commands={} skills={}  {}",
            name,
            version,
            commands,
            skills,
            manifest_path.display()
        ));
    }

    let mut out = String::new();
    let _ = writeln!(out, "Plugin");
    if rows.is_empty() {
        let _ = writeln!(out, "  <none found>");
    } else {
        let _ = writeln!(out, "  found={}", rows.len());
        for row in rows.into_iter().take(30) {
            let _ = writeln!(out, "{row}");
        }
    }
    Ok(out)
}

fn render_skill_overview() -> String {
    let mut roots = Vec::new();
    if let Some(home) = home_dir() {
        roots.push(("agents", home.join(".agents").join("skills")));
        roots.push(("claude", home.join(".claude").join("skills")));
        roots.push(("codex", home.join(".codex").join("skills")));
    }

    let mut out = String::new();
    let _ = writeln!(out, "Skill");
    for (label, root) in roots {
        let entries = fs::read_dir(&root)
            .ok()
            .into_iter()
            .flat_map(std::iter::Iterator::flatten)
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        let _ = writeln!(out, "  {}: {}  {}", label, entries.len(), root.display());
        for name in entries.into_iter().take(12) {
            let _ = writeln!(out, "    - {name}");
        }
    }
    out
}

fn render_mcp_overview() -> String {
    let mut out = String::new();
    let _ = writeln!(out, "MCP");
    let mut files = Vec::new();
    if let Some(home) = home_dir() {
        files.push(home.join(".claude").join("config.json"));
        files.push(home.join(".claude").join("settings.json"));
        files.push(home.join(".claude").join("settings.local.json"));
    }

    let mut discovered = BTreeMap::<String, usize>::new();
    for path in files {
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        for token in extract_mcp_server_tokens(&content) {
            *discovered.entry(token).or_default() += 1;
        }
    }

    if discovered.is_empty() {
        let _ = writeln!(
            out,
            "  No MCP server names were inferred from local Claude config files."
        );
        let _ = writeln!(
            out,
            "  Checked ~/.claude/config.json, settings.json, settings.local.json"
        );
        return out;
    }

    let _ = writeln!(
        out,
        "  discovered={}  source=~/.claude/*.json",
        discovered.len()
    );
    for (name, hits) in discovered {
        let _ = writeln!(out, "  - {name}  references={hits}");
    }
    out
}

fn extract_mcp_server_tokens(content: &str) -> Vec<String> {
    let mut names = std::collections::BTreeSet::<String>::new();
    let needle = "mcp__";
    let mut start = 0usize;
    while let Some(found) = content[start..].find(needle) {
        let absolute = start + found + needle.len();
        let remainder = &content[absolute..];
        let Some(end) = remainder.find("__") else {
            break;
        };
        let candidate = &remainder[..end];
        if !candidate.is_empty()
            && candidate
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        {
            names.insert(candidate.to_string());
        }
        start = absolute + end + 2;
    }
    names.into_iter().collect()
}

fn run_one_shot_prompt(
    state: &ChatSessionState,
    prompt: &str,
    runtime: &tokio::runtime::Runtime,
    model_override: Option<&str>,
    extra_instruction: Option<&str>,
) -> Result<ChatTurnOutput, String> {
    let requested_model = model_override.unwrap_or(&state.model);
    let resolved_model = resolve_model_alias(requested_model);
    let provider_kind = detect_provider_kind(&resolved_model);
    let prepared = prepare_ai_provider(provider_kind, &resolved_model)?;
    let client = prepared.client;
    let max_tokens = if state.fast_mode {
        state
            .max_tokens
            .unwrap_or_else(|| max_tokens_for_model(&resolved_model).min(1024))
            .min(768)
    } else {
        state
            .max_tokens
            .unwrap_or_else(|| max_tokens_for_model(&resolved_model).min(2048))
    };
    let mut system_prompt = build_chat_system_prompt(
        state.system.as_deref(),
        &state.project_context,
        state.fast_mode,
    );
    if let Some(extra_instruction) = extra_instruction
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        system_prompt.push_str("\n\n# Command instruction\n");
        system_prompt.push_str(extra_instruction);
    }

    let request = MessageRequest {
        model: resolved_model.clone(),
        max_tokens,
        messages: vec![InputMessage::user_text(prompt.to_string())],
        system: Some(system_prompt),
        tools: None,
        tool_choice: None,
        stream: false,
    };
    let response = runtime
        .block_on(client.send_message(&request))
        .map_err(|error| format!("AI request failed: {error}"))?;
    let response_text = extract_response_text(&response.content);

    Ok(ChatTurnOutput {
        response_text: if response_text.trim().is_empty() {
            "<no text response>".to_string()
        } else {
            response_text
        },
        input_tokens: response.usage.input_tokens,
        output_tokens: response.usage.output_tokens,
        tool_calls: 0,
        request_id: response.request_id.clone(),
    })
}

fn handle_shell_bang(
    shell_command: &str,
    state: &mut ChatSessionState,
    out: &mut impl Write,
) -> Result<ChatLoopControl, String> {
    let (stdout, stderr, exit_code) = run_shell_capture(shell_command, &state.project_context.cwd)?;
    let mut rendered = String::new();
    let _ = writeln!(rendered, "! {shell_command}");
    let _ = writeln!(rendered, "exit_code={exit_code}");
    if !stdout.trim().is_empty() {
        let _ = writeln!(
            rendered,
            "\nstdout:\n{}",
            truncate_for_display(stdout.trim(), 8_000)
        );
    }
    if !stderr.trim().is_empty() {
        let _ = writeln!(
            rendered,
            "\nstderr:\n{}",
            truncate_for_display(stderr.trim(), 4_000)
        );
    }

    writeln!(out, "{}", truncate_for_display(&rendered, 8_000)).map_err(stringify_error)?;
    state.messages.push(InputMessage::user_text(format!(
        "Shell command output captured for context:\n{}",
        truncate_for_display(&rendered, 8_000)
    )));
    save_chat_session(state)?;
    Ok(ChatLoopControl::Continue)
}

fn run_chat_turn(
    state: &mut ChatSessionState,
    user_input: &str,
    out: &mut impl Write,
    runtime: &tokio::runtime::Runtime,
) -> Result<ChatTurnOutput, String> {
    let resolved_model = resolve_model_alias(&state.model);
    let provider_kind = detect_provider_kind(&resolved_model);
    let prepared = match prepare_ai_provider(provider_kind, &resolved_model) {
        Ok(val) => val,
        Err(e) => {
            return Err(format!(
                "API credentials not configured for `{resolved_model}`. \
                 Run `stats-code auth set` to add your key.\nDetails: {e}"
            ));
        }
    };
    let client = prepared.client;
    let mut messages = state.messages.clone();
    messages.push(InputMessage::user_text(user_input.to_string()));

    let max_tokens = if state.fast_mode {
        state
            .max_tokens
            .unwrap_or_else(|| max_tokens_for_model(&resolved_model).min(1024))
            .min(768)
    } else {
        state
            .max_tokens
            .unwrap_or_else(|| max_tokens_for_model(&resolved_model).min(4096))
    };
    let system_prompt = build_chat_system_prompt(
        state.system.as_deref(),
        &state.project_context,
        state.fast_mode,
    );
    let tools = if state.use_tools {
        Some(chat_tool_definitions())
    } else {
        None
    };
    let tool_choice = if state.use_tools {
        Some(ToolChoice::Auto)
    } else {
        None
    };
    let mut input_tokens = 0u32;
    let mut output_tokens = 0u32;
    let mut tool_calls = 0usize;

    // A6: Configurable tool call round limit
    const MAX_TOOL_ROUNDS: usize = 12;
    for _ in 0..MAX_TOOL_ROUNDS {
        let request = MessageRequest {
            model: resolved_model.clone(),
            max_tokens,
            messages: messages.clone(),
            system: Some(system_prompt.clone()),
            tools: tools.clone(),
            tool_choice: tool_choice.clone(),
            stream: false,
        };
        let response = match runtime.block_on(client.send_message(&request)) {
            Ok(r) => r,
            Err(e) => {
                return Err(format!(
                    "Request to `{resolved_model}` failed: {e}\n\
                     Check your network connection and API key. \
                     You can retry by sending your message again."
                ));
            }
        };
        input_tokens = input_tokens.saturating_add(response.usage.input_tokens);
        output_tokens = output_tokens.saturating_add(response.usage.output_tokens);

        let assistant_message = assistant_message_from_response(&response.content);
        if !assistant_message.content.is_empty() {
            messages.push(assistant_message);
        }

        let pending_tool_uses = collect_pending_tool_uses(&response.content);
        if pending_tool_uses.is_empty() {
            let response_text = extract_response_text(&response.content);
            state.messages = messages;
            record_session_usage(
                state,
                input_tokens,
                output_tokens,
                tool_calls,
                response.request_id.clone(),
            );
            save_chat_session(state)?;
            return Ok(ChatTurnOutput {
                response_text: if response_text.trim().is_empty() {
                    "<no text response>".to_string()
                } else {
                    response_text
                },
                input_tokens,
                output_tokens,
                tool_calls,
                request_id: response.request_id.clone(),
            });
        }

        for tool_use in pending_tool_uses {
            tool_calls += 1;
            // P1 UX4: Show only filename, not full path
            let tool_summary = summarize_tool_input_short(&tool_use.input);
            write!(
                out,
                "  {} {}({}) {}",
                "\u{29bf}".truecolor(140, 140, 130),
                tool_use.name.truecolor(180, 140, 80),
                tool_summary,
                "...".truecolor(100, 100, 100)
            )
            .map_err(stringify_error)?;
            out.flush().map_err(stringify_error)?;
            // P1 UX1: Time the tool execution
            let tool_start = std::time::Instant::now();
            let (tool_output, is_error) = match execute_chat_tool(
                &tool_use.name,
                &tool_use.input,
                state.artifacts_dir.as_deref(),
            ) {
                Ok(output) => (output, false),
                Err(error) => (error, true),
            };
            let elapsed = tool_start.elapsed();
            // Overwrite the '...' progress line with final result
            write!(out, "\r").map_err(stringify_error)?;
            writeln!(
                out,
                "  {} {}({}) {} {:.2}s",
                if is_error {
                    "\u{2716}".truecolor(220, 60, 60)
                } else {
                    "\u{2714}".truecolor(80, 180, 80)
                },
                tool_use.name,
                tool_summary,
                "\u{00b7}".truecolor(100, 100, 100),
                elapsed.as_secs_f32()
            )
            .map_err(stringify_error)?;
            messages.push(InputMessage::user_tool_result(
                tool_use.id,
                tool_output,
                is_error,
            ));
        }
    }

    Err(
        "The model used more than 12 consecutive tool calls without a final answer. \
         Try rephrasing your request, or use /tools off to disable tool calling."
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
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
        fs::create_dir_all(project_root.join(".claude").join("commands"))
            .expect("project commands");
        fs::create_dir_all(
            project_root
                .join(".claude")
                .join("plugins")
                .join("demo")
                .join(".claude-plugin")
                .join("commands"),
        )
        .expect("plugin commands");
        fs::create_dir_all(user_home.join(".claude").join("commands")).expect("user commands");
        fs::write(
            project_root
                .join(".claude")
                .join("commands")
                .join("project.md"),
            "---\ndescription: Project command\n---\nproject body",
        )
        .expect("write project command");
        fs::write(
            project_root
                .join(".claude")
                .join("plugins")
                .join("demo")
                .join(".claude-plugin")
                .join("plugin.json"),
            r#"{"name":"demo"}"#,
        )
        .expect("write plugin manifest");
        fs::write(
            project_root
                .join(".claude")
                .join("plugins")
                .join("demo")
                .join(".claude-plugin")
                .join("commands")
                .join("plugin-cmd.md"),
            "---\ndescription: Plugin command\n---\nplugin body",
        )
        .expect("write plugin command");
        fs::write(
            user_home.join(".claude").join("commands").join("user.md"),
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
            .any(|command| command.source.contains("project .claude/commands")));
        assert!(commands
            .iter()
            .any(|command| command.source.contains("user ~/.claude/commands")));
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
}
