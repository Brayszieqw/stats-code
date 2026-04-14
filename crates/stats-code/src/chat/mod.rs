mod context;
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
use serde_json::Value;
use colored::Colorize;

use crate::cli::{
    AuthSetArgs, ChatArgs, Cli, ConfigModelArgs,
};
use crate::config::{
    current_stats_code_profile, estimate_session_cost_usd, extract_response_text,
    handle_auth_set, handle_config_add_model, handle_config_default_model,
    handle_config_remove_model, handle_config_show,
    home_dir, load_auth_store, load_stats_code_settings,
    parse_auth_provider_name, prepare_ai_provider, resolve_requested_model,
    save_auth_store, save_stats_code_settings, stats_code_auth_path,
    stats_code_env_path, stats_code_profile_path,
    stats_code_settings_path, ChatUsageTotals, ModelPricing,
};
use crate::input::{LineEditor, ReadOutcome};
use crate::render::{
    render_auth_set_text, render_config_text,
};
use crate::ui::{ChatEntryKind, ChatUi, ChatUiStatus};
use crate::helpers::{stringify_error, unix_timestamp_nanos};

// Re-exports from sub-modules
use self::context::{build_chat_system_prompt, collect_project_context, format_project_context_summary};
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
#[allow(dead_code)]
pub(crate) struct ChatTurnOutput {
    response_text: String,
    resolved_model: String,
    credential_source: String,
    input_tokens: u32,
    output_tokens: u32,
    total_tokens: u32,
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
    BuiltinSlashCommand { name: "help", args: "", description_zh: "\u{663e}\u{793a}\u{5e2e}\u{52a9}\u{4fe1}\u{606f}" },
    BuiltinSlashCommand { name: "clear", args: "", description_zh: "\u{6e05}\u{9664}\u{5bf9}\u{8bdd}\u{5386}\u{53f2}" },
    BuiltinSlashCommand { name: "compact", args: "", description_zh: "\u{538b}\u{7f29}\u{5bf9}\u{8bdd}\u{4e0a}\u{4e0b}\u{6587}\u{ff08}\u{8282}\u{7701} token\u{ff09}" },
    BuiltinSlashCommand { name: "cost", args: "", description_zh: "\u{663e}\u{793a}\u{5f53}\u{524d}\u{4f1a}\u{8bdd}\u{7684} token \u{7528}\u{91cf}\u{548c}\u{8d39}\u{7528}" },
    BuiltinSlashCommand { name: "status", args: "", description_zh: "\u{663e}\u{793a}\u{5f53}\u{524d}\u{914d}\u{7f6e}\u{72b6}\u{6001}" },
    BuiltinSlashCommand { name: "model", args: "", description_zh: "\u{5207}\u{6362}\u{6a21}\u{578b}\u{ff08}\u{5982} sonnet/opus/haiku\u{ff09}" },
    BuiltinSlashCommand { name: "fast", args: "", description_zh: "\u{5207}\u{6362} Fast \u{6a21}\u{5f0f}\u{ff08}\u{66f4}\u{5feb}\u{8f93}\u{51fa}\u{ff09}" },
    BuiltinSlashCommand { name: "memory", args: "", description_zh: "\u{67e5}\u{770b}/\u{7f16}\u{8f91}\u{8bb0}\u{5fc6}\u{6587}\u{4ef6}" },
    BuiltinSlashCommand { name: "config", args: "", description_zh: "\u{67e5}\u{770b}\u{6216}\u{4fee}\u{6539}\u{914d}\u{7f6e}" },
    BuiltinSlashCommand { name: "review", args: "", description_zh: "\u{4ee3}\u{7801}\u{5ba1}\u{67e5}" },
    BuiltinSlashCommand { name: "pr_comments", args: "", description_zh: "\u{67e5}\u{770b} PR \u{8bc4}\u{8bba}" },
    BuiltinSlashCommand { name: "init", args: "", description_zh: "\u{521d}\u{59cb}\u{5316}\u{9879}\u{76ee}\u{ff08}\u{751f}\u{6210} CLAUDE.md\u{ff09}" },
    BuiltinSlashCommand { name: "login", args: "", description_zh: "\u{767b}\u{5f55}\u{8d26}\u{53f7}/\u{4fdd}\u{5b58}\u{51ed}\u{636e}" },
    BuiltinSlashCommand { name: "logout", args: "", description_zh: "\u{9000}\u{51fa}\u{767b}\u{5f55}/\u{79fb}\u{9664}\u{51ed}\u{636e}" },
    BuiltinSlashCommand { name: "bug", args: "", description_zh: "\u{62a5}\u{544a} Stats Code \u{7684} bug" },
    BuiltinSlashCommand { name: "release-notes", args: "", description_zh: "\u{67e5}\u{770b}\u{66f4}\u{65b0}\u{65e5}\u{5fd7}" },
    BuiltinSlashCommand { name: "vim", args: "", description_zh: "\u{5207}\u{6362} Vim \u{8f93}\u{5165}\u{6a21}\u{5f0f}" },
    BuiltinSlashCommand { name: "terminal-setup", args: "", description_zh: "\u{914d}\u{7f6e}\u{7ec8}\u{7aef}\u{ff08}shift+enter \u{6362}\u{884c}\u{7b49}\u{ff09}" },
    BuiltinSlashCommand { name: "plugin", args: "", description_zh: "\u{67e5}\u{770b}\u{63d2}\u{4ef6}" },
    BuiltinSlashCommand { name: "skill", args: "", description_zh: "\u{67e5}\u{770b}\u{6280}\u{80fd}" },
    BuiltinSlashCommand { name: "mcp", args: "", description_zh: "\u{67e5}\u{770b} MCP \u{914d}\u{7f6e}" },
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
        slash_command_completion_candidates(&state.project_context.cwd)?,
    );

    ui.print_welcome(&mut stdout, &build_chat_ui_status(&state)).map_err(stringify_error)?;

    if state.session_loaded {
        ui.print_turn(&mut stdout, ChatEntryKind::System, &format!(
            "Resumed session from {} with {} stored messages.",
            state.session_path.display(),
            state.messages.len()
        )).map_err(stringify_error)?;
    } else {
        ui.print_turn(&mut stdout, ChatEntryKind::System, &format!(
            "Workspace {} ready. Type `/` for commands or `!` for shell.",
            state.project_context.cwd.display()
        )).map_err(stringify_error)?;
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
        ui.print_input_bottom(&mut stdout).map_err(stringify_error)?;


        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }
        editor.push_history(trimmed.to_string());

        if let Some(shell_command) = trimmed.strip_prefix('!').map(str::trim) {
            if shell_command.is_empty() {
                ui.print_turn(&mut stdout, ChatEntryKind::Error, "Missing shell command after `!`.").map_err(stringify_error)?;
                continue;
            }
            ui.print_turn(&mut stdout, ChatEntryKind::User, &format!("! {shell_command}")).map_err(stringify_error)?;
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
                    ui.print_turn(&mut stdout, ChatEntryKind::Error, &err).map_err(stringify_error)?;
                }
            }
            continue;
        }

        if trimmed.starts_with('/') {
            ui.print_turn(&mut stdout, ChatEntryKind::User, trimmed).map_err(stringify_error)?;
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
                        crossterm::execute!(&mut stdout, crossterm::terminal::Clear(crossterm::terminal::ClearType::All)).map_err(stringify_error)?;
                    }
                    print_ui_output(&ui, &mut stdout, ChatEntryKind::System, &output);
                }
                Err(err) => {
                    print_ui_output(&ui, &mut stdout, ChatEntryKind::System, &output);
                    ui.print_turn(&mut stdout, ChatEntryKind::Error, &err).map_err(stringify_error)?;
                }
            }
            continue;
        }

        ui.print_turn(&mut stdout, ChatEntryKind::User, trimmed).map_err(stringify_error)?;
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
                    &format!("{err}\nCheck your API key with `stats-code auth status`, or type /help.")
                ).map_err(stringify_error)?;
                continue;
            }
        };
        print_ui_output(&ui, &mut stdout, ChatEntryKind::Tool, &tool_output);
        ui.print_turn(&mut stdout, ChatEntryKind::Assistant, turn.response_text.trim()).map_err(stringify_error)?;
    }

    Ok(())
}

fn build_chat_ui_status(state: &ChatSessionState) -> ChatUiStatus {
    let resolved_model = resolve_model_alias(&state.model);
    let estimated_cost_usd = load_stats_code_settings(&stats_code_settings_path())
        .ok()
        .and_then(|settings| estimate_session_cost_usd(&settings.pricing, &resolved_model, &state.usage));

    ChatUiStatus {
        model: resolved_model,
        workspace: state.project_context.cwd.display().to_string(),
        tools_enabled: state.use_tools,
        fast_mode: state.fast_mode,
        vim_mode: state.vim_mode,
        turns: state.usage.turns.min(usize::MAX as u64) as usize,
        input_tokens: state.usage.input_tokens.min(u64::from(u32::MAX)) as u32,
        output_tokens: state.usage.output_tokens.min(u64::from(u32::MAX)) as u32,
        estimated_cost_usd,
        session_loaded: state.session_loaded,
    }
}

fn print_ui_output(ui: &ChatUi, out: &mut impl Write, kind: ChatEntryKind, output: &[u8]) {
    if output.is_empty() {
        return;
    }
    let rendered = String::from_utf8_lossy(output).trim().to_string();
    if !rendered.is_empty() {
        let _ = ui.print_turn(out, kind, &rendered);
    }
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
    state.usage.tool_calls = state
        .usage
        .tool_calls
        .saturating_add(tool_calls as u64);
    state.usage.turns = state.usage.turns.saturating_add(1);
    state.last_request_id = request_id;
}

fn truncate_for_display(content: impl AsRef<str>, max_chars: usize) -> String {
    let content = content.as_ref();
    if content.chars().count() <= max_chars {
        return content.to_string();
    }

    let mut result = String::new();
    for ch in content.chars().take(max_chars) {
        result.push(ch);
    }
    result.push_str("\n... [truncated]");
    result
}

fn format_token_count(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.2}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}


fn primary_memory_file_path(cwd: &Path) -> PathBuf {
    let stats_md = cwd.join("STATS.md");
    if stats_md.is_file() {
        return stats_md;
    }
    stats_md
}

fn append_memory_note(path: &Path, note: &str) -> Result<(), String> {
    let trimmed = note.trim();
    if trimmed.is_empty() {
        return Err("Usage: /memory add <text>".to_string());
    }

    let existing = fs::read_to_string(path).unwrap_or_default();
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    if !updated.contains("## Session Memory") {
        if !updated.is_empty() {
            updated.push('\n');
        }
        updated.push_str("## Session Memory\n");
    }
    updated.push_str(&format!("- {trimmed}\n"));

    fs::write(path, updated).map_err(stringify_error)
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
    let _ = writeln!(out, "  Project cwd       {}", state.project_context.cwd.display());
    let _ = writeln!(out, "  Model             {} -> {}", state.model, resolved_model);
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
    state.messages.push(InputMessage::user_text(user_text.into()));
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

fn collect_markdown_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if !root.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(root).map_err(stringify_error)? {
        let entry = entry.map_err(stringify_error)?;
        let path = entry.path();
        if path.is_dir() {
            collect_markdown_files(&path, files)?;
        } else if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("md"))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn parse_command_template(contents: &str) -> (Option<String>, String) {
    let trimmed = contents.trim_start_matches('\u{feff}');
    if let Some(rest) = trimmed.strip_prefix("---\n") {
        if let Some((frontmatter, body)) = rest.split_once("\n---\n") {
            let description = frontmatter
                .lines()
                .find_map(|line| line.trim().strip_prefix("description:"))
                .map(str::trim)
                .map(|value| value.trim_matches('"').trim_matches('\'').to_string())
                .filter(|value| !value.is_empty());
            return (description, body.trim().to_string());
        }
    }
    (None, trimmed.trim().to_string())
}

fn command_name_from_relative_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let stem = relative.with_extension("");
    let name = stem
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("/");
    if name.is_empty() || name.eq_ignore_ascii_case("README") {
        None
    } else {
        Some(name)
    }
}

fn nearest_project_claude_dir(cwd: &Path) -> Option<PathBuf> {
    cwd.ancestors()
        .map(|ancestor| ancestor.join(".claude"))
        .find(|path| path.is_dir())
}

fn plugin_command_roots(plugins_dir: &Path) -> Result<Vec<(PathBuf, String)>, String> {
    let mut manifests = Vec::new();
    collect_plugin_manifests(plugins_dir, &mut manifests)?;

    let mut roots = BTreeMap::new();
    for manifest_path in manifests {
        let manifest_dir = match manifest_path.parent() {
            Some(parent) => parent,
            None => continue,
        };
        let plugin_root = if manifest_dir
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value == ".claude-plugin")
        {
            manifest_dir
                .parent().map_or_else(|| manifest_dir.to_path_buf(), Path::to_path_buf)
        } else {
            manifest_dir.to_path_buf()
        };
        let plugin_name = fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .and_then(|value| value.get("name").and_then(Value::as_str).map(ToOwned::to_owned))
            .or_else(|| {
                plugin_root
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| "plugin".to_string());

        for candidate in [
            plugin_root.join("commands"),
            plugin_root.join(".claude-plugin").join("commands"),
        ] {
            if candidate.is_dir() {
                roots.entry(candidate).or_insert_with(|| plugin_name.clone());
            }
        }
    }

    Ok(roots.into_iter().collect())
}

fn collect_plugin_manifests(root: &Path, manifests: &mut Vec<PathBuf>) -> Result<(), String> {
    if !root.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(root).map_err(stringify_error)? {
        let entry = entry.map_err(stringify_error)?;
        let path = entry.path();
        if path.is_dir() {
            collect_plugin_manifests(&path, manifests)?;
        } else if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("plugin.json"))
        {
            manifests.push(path);
        }
    }
    Ok(())
}

pub(crate) fn discover_slash_command_templates(cwd: &Path) -> Result<Vec<SlashCommandTemplate>, String> {
    let mut discovered = BTreeMap::<String, SlashCommandTemplate>::new();

    if let Some(project_claude) = nearest_project_claude_dir(cwd) {
        let commands_root = project_claude.join("commands");
        let mut files = Vec::new();
        collect_markdown_files(&commands_root, &mut files)?;
        for path in files {
            let Some(name) = command_name_from_relative_path(&commands_root, &path) else {
                continue;
            };
            let (description, body) = parse_command_template(
                &fs::read_to_string(&path).map_err(stringify_error)?,
            );
            discovered.entry(name.clone()).or_insert(SlashCommandTemplate {
                name,
                _description: description,
                body,
                path,
                source: "project .claude/commands".to_string(),
            });
        }
    }

    if let Some(home) = home_dir() {
        let user_commands_root = home.join(".claude").join("commands");
        let mut files = Vec::new();
        collect_markdown_files(&user_commands_root, &mut files)?;
        for path in files {
            let Some(name) = command_name_from_relative_path(&user_commands_root, &path) else {
                continue;
            };
            let (description, body) = parse_command_template(
                &fs::read_to_string(&path).map_err(stringify_error)?,
            );
            discovered.entry(name.clone()).or_insert(SlashCommandTemplate {
                name,
                _description: description,
                body,
                path,
                source: "user ~/.claude/commands".to_string(),
            });
        }

        let user_plugins_root = home.join(".claude").join("plugins");
        for (commands_root, plugin_name) in plugin_command_roots(&user_plugins_root)? {
            let mut files = Vec::new();
            collect_markdown_files(&commands_root, &mut files)?;
            for path in files {
                let Some(name) = command_name_from_relative_path(&commands_root, &path) else {
                    continue;
                };
                let (description, body) = parse_command_template(
                    &fs::read_to_string(&path).map_err(stringify_error)?,
                );
                discovered.entry(name.clone()).or_insert(SlashCommandTemplate {
                    name,
                    _description: description,
                    body,
                    path,
                    source: format!("plugin:{plugin_name}"),
                });
            }
        }
    }

    if let Some(project_claude) = nearest_project_claude_dir(cwd) {
        let project_plugins_root = project_claude.join("plugins");
        for (commands_root, plugin_name) in plugin_command_roots(&project_plugins_root)? {
            let mut files = Vec::new();
            collect_markdown_files(&commands_root, &mut files)?;
            for path in files {
                let Some(name) = command_name_from_relative_path(&commands_root, &path) else {
                    continue;
                };
                let (description, body) = parse_command_template(
                    &fs::read_to_string(&path).map_err(stringify_error)?,
                );
                discovered.insert(
                    name.clone(),
                    SlashCommandTemplate {
                        name,
                        _description: description,
                        body,
                        path,
                        source: format!("project-plugin:{plugin_name}"),
                    },
                );
            }
        }
    }

    Ok(discovered.into_values().collect())
}

pub(crate) fn slash_command_completion_candidates(cwd: &Path) -> Result<Vec<String>, String> {
    let _ = cwd;
    Ok(BUILTIN_SLASH_COMMANDS
        .iter()
        .map(|command| format!("/{}", command.name))
        .collect())
}

fn render_custom_command_prompt(template: &SlashCommandTemplate, args: &str, cwd: &Path) -> String {
    let rendered_body = template
        .body
        .replace("$ARGUMENTS", args)
        .replace("{{args}}", args)
        .replace("$CWD", &cwd.display().to_string());

    format!(
        "Execute the slash command `/{}`

Source: {}
Path: {}
Working directory: {}

Command instructions:
{}

User arguments:
{}",
        template.name,
        template.source,
        template.path.display(),
        cwd.display(),
        rendered_body,
        if args.trim().is_empty() { "<none>" } else { args.trim() }
    )
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
        let _ = writeln!(
            out,
            "│{:<15}   │{:<32}  │",
            label,
            command.description_zh
        );
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
        let name = json
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("plugin");
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
        let _ = writeln!(out, "  No MCP server names were inferred from local Claude config files.");
        let _ = writeln!(out, "  Checked ~/.claude/config.json, settings.json, settings.local.json");
        return out;
    }

    let _ = writeln!(out, "  discovered={}  source=~/.claude/*.json", discovered.len());
    for (name, hits) in discovered {
        let _ = writeln!(out, "  - {name}  references={hits}");
    }
    out
}

fn extract_mcp_server_tokens(content: &str) -> Vec<String> {
    let mut names = BTreeMap::<String, ()>::new();
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
            names.insert(candidate.to_string(), ());
        }
        start = absolute + end + 2;
    }
    names.into_keys().collect()
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
    let credential_source = prepared.credential_source.clone();
    let client = prepared.client;
    let max_tokens = if state.fast_mode {
        state.max_tokens
            .unwrap_or_else(|| max_tokens_for_model(&resolved_model).min(1024))
            .min(768)
    } else {
        state.max_tokens
            .unwrap_or_else(|| max_tokens_for_model(&resolved_model).min(2048))
    };
    let mut system_prompt =
        build_chat_system_prompt(state.system.as_deref(), &state.project_context, state.fast_mode);
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
        resolved_model,
        credential_source,
        input_tokens: response.usage.input_tokens,
        output_tokens: response.usage.output_tokens,
        total_tokens: response.total_tokens(),
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
        let _ = writeln!(rendered, "\nstdout:\n{}", truncate_for_display(stdout.trim(), 8_000));
    }
    if !stderr.trim().is_empty() {
        let _ = writeln!(rendered, "\nstderr:\n{}", truncate_for_display(stderr.trim(), 4_000));
    }

    writeln!(out, "{}", truncate_for_display(&rendered, 8_000)).map_err(stringify_error)?;
    state.messages.push(InputMessage::user_text(format!(
        "Shell command output captured for context:\n{}",
        truncate_for_display(&rendered, 8_000)
    )));
    save_chat_session(state)?;
    Ok(ChatLoopControl::Continue)
}


pub(crate) fn handle_chat_command(
    input: &str,
    state: &mut ChatSessionState,
    out: &mut impl Write,
    runtime: &tokio::runtime::Runtime,
) -> Result<ChatLoopControl, String> {
    if input.trim() == "/" || input.trim() == "/?" {
        return handle_chat_command("/help", state, out, runtime);
    }

    let command = input.trim().trim_start_matches('/');
    let mut parts = command.split_whitespace();
    let Some(name) = parts.next() else {
        return Ok(ChatLoopControl::Continue);
    };
    let args = command
        .strip_prefix(name)
        .map(str::trim)
        .unwrap_or_default();

    if name == "status" {
        writeln!(out, "{}", render_status_report(state)?).map_err(stringify_error)?;
        return Ok(ChatLoopControl::Continue);
    }

    if name == "cost" {
        let settings = load_stats_code_settings(&stats_code_settings_path())?;
        let resolved_model = resolve_model_alias(&state.model);
        writeln!(out, "Cost").map_err(stringify_error)?;
        writeln!(out, "  Model             {} -> {}", state.model, resolved_model)
            .map_err(stringify_error)?;
        writeln!(
            out,
            "  Input tokens      {}",
            format_token_count(state.usage.input_tokens)
        )
        .map_err(stringify_error)?;
        writeln!(
            out,
            "  Output tokens     {}",
            format_token_count(state.usage.output_tokens)
        )
        .map_err(stringify_error)?;
        writeln!(
            out,
            "  Total tokens      {}",
            format_token_count(state.usage.input_tokens + state.usage.output_tokens)
        )
        .map_err(stringify_error)?;
        match estimate_session_cost_usd(&settings.pricing, &resolved_model, &state.usage) {
            Some(cost) => writeln!(out, "  Estimated cost    ${cost:.4}").map_err(stringify_error)?,
            None => writeln!(
                out,
                "  Estimated cost    unavailable (configure with /config pricing <model> <input_usd_per_1m> <output_usd_per_1m>)"
            )
            .map_err(stringify_error)?,
        }
        return Ok(ChatLoopControl::Continue);
    }

    if name == "fast" {
        match args {
            "" => state.fast_mode = !state.fast_mode,
            "on" => state.fast_mode = true,
            "off" => state.fast_mode = false,
            _ => return Err("Usage: /fast [on|off]".to_string()),
        }
        save_chat_session(state)?;
        writeln!(
            out,
            "Fast mode is {}.",
            if state.fast_mode { "enabled" } else { "disabled" }
        )
        .map_err(stringify_error)?;
        return Ok(ChatLoopControl::Continue);
    }

    if name == "vim" {
        match args {
            "" => state.vim_mode = !state.vim_mode,
            "on" => state.vim_mode = true,
            "off" => state.vim_mode = false,
            _ => return Err("Usage: /vim [on|off]".to_string()),
        }
        save_chat_session(state)?;
        writeln!(
            out,
            "Vim mode preference is {}. Current REPL still uses the standard console input path.",
            if state.vim_mode { "enabled" } else { "disabled" }
        )
        .map_err(stringify_error)?;
        return Ok(ChatLoopControl::Continue);
    }

    if name == "memory" {
        let mut parts = args.split_whitespace();
        match parts.next() {
            None | Some("show") => {
                let path = primary_memory_file_path(&state.project_context.cwd);
                let content = fs::read_to_string(&path).unwrap_or_default();
                writeln!(out, "Memory").map_err(stringify_error)?;
                writeln!(out, "  File   {}", path.display()).map_err(stringify_error)?;
                writeln!(
                    out,
                    "  Loaded {}",
                    format_project_context_summary(&state.project_context)
                )
                .map_err(stringify_error)?;
                if content.trim().is_empty() {
                    writeln!(out, "  Content <empty>").map_err(stringify_error)?;
                } else {
                    writeln!(out).map_err(stringify_error)?;
                    writeln!(out, "{}", truncate_for_display(content, 4_000))
                        .map_err(stringify_error)?;
                }
            }
            Some("add") => {
                let note = parts.collect::<Vec<_>>().join(" ");
                let path = primary_memory_file_path(&state.project_context.cwd);
                append_memory_note(&path, &note)?;
                state.project_context = collect_project_context(&state.project_context.cwd)?;
                save_chat_session(state)?;
                writeln!(out, "Updated {}", path.display()).map_err(stringify_error)?;
            }
            Some("reload") => {
                state.project_context = collect_project_context(&state.project_context.cwd)?;
                save_chat_session(state)?;
                writeln!(
                    out,
                    "Memory reloaded: {}",
                    format_project_context_summary(&state.project_context)
                )
                .map_err(stringify_error)?;
            }
            Some(other) => {
                return Err(format!(
                    "Unknown `/memory` action `{other}`. Use `/memory`, `/memory add <text>`, or `/memory reload`."
                ));
            }
        }
        return Ok(ChatLoopControl::Continue);
    }

    if name == "config" {
        let mut parts = args.split_whitespace();
        match parts.next() {
            None | Some("show") => {
                writeln!(out, "{}", render_config_text(&handle_config_show()?))
                    .map_err(stringify_error)?;
            }
            Some("env") => {
                writeln!(out, "Config paths").map_err(stringify_error)?;
                writeln!(out, "  settings  {}", stats_code_settings_path().display())
                    .map_err(stringify_error)?;
                writeln!(out, "  profile   {}", stats_code_profile_path().display())
                    .map_err(stringify_error)?;
                writeln!(out, "  env       {}", stats_code_env_path().display())
                    .map_err(stringify_error)?;
                writeln!(out, "  auth      {}", stats_code_auth_path().display())
                    .map_err(stringify_error)?;
            }
            Some("model") => {
                let Some(model) = parts.next() else {
                    return Err("Usage: /config model <name>".to_string());
                };
                let result = handle_config_default_model(&ConfigModelArgs {
                    model: model.to_string(),
                })?;
                writeln!(out, "{}", result.message).map_err(stringify_error)?;
            }
            Some("pricing") => {
                let maybe_model = parts.next();
                let maybe_input = parts.next();
                let maybe_output = parts.next();
                if maybe_model.is_none() {
                    let settings = load_stats_code_settings(&stats_code_settings_path())?;
                    writeln!(out, "Configured pricing").map_err(stringify_error)?;
                    if settings.pricing.is_empty() {
                        writeln!(out, "  <none>").map_err(stringify_error)?;
                    } else {
                        for (model, pricing) in settings.pricing {
                            writeln!(
                                out,
                                "  {}  input=${:.4}/1M output=${:.4}/1M",
                                model, pricing.input_per_million_usd, pricing.output_per_million_usd
                            )
                            .map_err(stringify_error)?;
                        }
                    }
                } else {
                    let model = maybe_model.unwrap_or_default();
                    let input_usd = maybe_input
                        .ok_or_else(|| "Usage: /config pricing <model> <input_usd_per_1m> <output_usd_per_1m>".to_string())?
                        .parse::<f64>()
                        .map_err(|_| "Input price must be a number.".to_string())?;
                    let output_usd = maybe_output
                        .ok_or_else(|| "Usage: /config pricing <model> <input_usd_per_1m> <output_usd_per_1m>".to_string())?
                        .parse::<f64>()
                        .map_err(|_| "Output price must be a number.".to_string())?;
                    let path = stats_code_settings_path();
                    let mut settings = load_stats_code_settings(&path)?;
                    settings.pricing.insert(
                        model.to_string(),
                        ModelPricing {
                            input_per_million_usd: input_usd,
                            output_per_million_usd: output_usd,
                        },
                    );
                    settings.updated_at_unix_nanos = unix_timestamp_nanos();
                    save_stats_code_settings(&path, &settings)?;
                    writeln!(
                        out,
                        "Configured pricing for {model}: input=${input_usd:.4}/1M output=${output_usd:.4}/1M"
                    )
                    .map_err(stringify_error)?;
                }
            }
            Some(other) => {
                return Err(format!(
                    "Unknown `/config` action `{other}`. Use `/config`, `/config env`, `/config model <name>`, or `/config pricing ...`."
                ));
            }
        }
        return Ok(ChatLoopControl::Continue);
    }

    if name == "review" {
        let cwd = &state.project_context.cwd;
        let (status_stdout, status_stderr, _) =
            run_process_capture("git", &["status", "--short"], cwd)?;
        let (diff_stdout, diff_stderr, _) = run_process_capture(
            "git",
            &["diff", "--minimal", "--no-ext-diff", "--no-color", "--unified=1"],
            cwd,
        )?;
        let review_material = format!(
            "Git status:\n{}\n\nGit diff:\n{}\n\nStatus stderr:\n{}\n\nDiff stderr:\n{}",
            if status_stdout.trim().is_empty() { "<clean>" } else { status_stdout.trim() },
            if diff_stdout.trim().is_empty() { "<no unstaged diff>" } else { diff_stdout.trim() },
            if status_stderr.trim().is_empty() { "<none>" } else { status_stderr.trim() },
            if diff_stderr.trim().is_empty() { "<none>" } else { diff_stderr.trim() },
        );
        let prompt = format!(
            "Review the current workspace changes.\n{}\n\nRespond with findings first, ordered by severity. Cite file paths and line numbers when possible. If you find no issues, say so explicitly and mention testing gaps or residual risk.",
            truncate_for_display(review_material, 18_000)
        );
        let review_model = current_stats_code_profile()
            .review_model
            .filter(|value| !value.trim().is_empty());
        let result = run_one_shot_prompt(
            state,
            &prompt,
            runtime,
            review_model.as_deref(),
            Some("Act as a strict code reviewer. Focus on bugs, regressions, unsafe assumptions, and missing tests."),
        )?;
        writeln!(out, "{}", result.response_text).map_err(stringify_error)?;
        record_session_usage(
            state,
            result.input_tokens,
            result.output_tokens,
            result.tool_calls,
            result.request_id.clone(),
        );
        append_chat_exchange(state, input.to_string(), result.response_text)?;
        return Ok(ChatLoopControl::Continue);
    }

    if name == "pr_comments" {
        let (stdout, stderr, exit_code) =
            run_process_capture("gh", &["pr", "view", "--comments"], &state.project_context.cwd)?;
        writeln!(out, "PR comments").map_err(stringify_error)?;
        writeln!(out, "  exit_code={exit_code}").map_err(stringify_error)?;
        if !stdout.trim().is_empty() {
            writeln!(out, "{}", truncate_for_display(stdout, 8_000)).map_err(stringify_error)?;
        }
        if !stderr.trim().is_empty() {
            writeln!(out, "stderr:\n{}", truncate_for_display(stderr, 2_000))
                .map_err(stringify_error)?;
        }
        return Ok(ChatLoopControl::Continue);
    }

    if name == "login" {
        let mut parts = args.split_whitespace();
        let Some(provider_name) = parts.next() else {
            writeln!(
                out,
                "Usage: /login <provider> <api-key> [base-url]\nProviders: openai, gemini, deepseek, dashscope, moonshot, xai"
            )
            .map_err(stringify_error)?;
            return Ok(ChatLoopControl::Continue);
        };
        let Some(provider) = parse_auth_provider_name(provider_name) else {
            return Err(format!("Unsupported provider `{provider_name}`."));
        };
        let Some(api_key) = parts.next() else {
            return Err("Usage: /login <provider> <api-key> [base-url]".to_string());
        };
        let base_url = parts.next().map(ToOwned::to_owned);
        let result = handle_auth_set(&AuthSetArgs {
            provider,
            api_key: api_key.to_string(),
            base_url,
        })?;
        writeln!(out, "{}", render_auth_set_text(&result)).map_err(stringify_error)?;
        return Ok(ChatLoopControl::Continue);
    }

    if name == "logout" {
        let Some(provider_name) = args.split_whitespace().next() else {
            return Err("Usage: /logout <provider>".to_string());
        };
        let Some(provider) = parse_auth_provider_name(provider_name) else {
            return Err(format!("Unsupported provider `{provider_name}`."));
        };
        let path = stats_code_auth_path();
        let mut store = load_auth_store(&path)?;
        let removed = store.providers.remove(provider.store_key()).is_some();
        save_auth_store(&path, &store)?;
        writeln!(
            out,
            "{} {}",
            if removed { "Removed saved credentials for" } else { "No saved credentials for" },
            provider.display_name()
        )
        .map_err(stringify_error)?;
        return Ok(ChatLoopControl::Continue);
    }

    if name == "bug" {
        let bug_path = state.project_context.cwd.join(format!(
            "stats-code-bug-report-{}.md",
            unix_timestamp_nanos()
        ));
        let report = format!(
            "# Stats Code Bug Report\n\n- version: {}\n- model: {}\n- session: {}\n- cwd: {}\n- fast_mode: {}\n- tools: {}\n- last_request_id: {}\n\n## Reproduction\n\nDescribe what happened.\n",
            env!("CARGO_PKG_VERSION"),
            state.model,
            state.session_path.display(),
            state.project_context.cwd.display(),
            state.fast_mode,
            state.use_tools,
            state.last_request_id.as_deref().unwrap_or("<none>")
        );
        fs::write(&bug_path, report).map_err(stringify_error)?;
        writeln!(out, "Created {}", bug_path.display()).map_err(stringify_error)?;
        return Ok(ChatLoopControl::Continue);
    }

    if name == "release-notes" {
        writeln!(out, "Release Notes").map_err(stringify_error)?;
        writeln!(out, "  Version  {}", env!("CARGO_PKG_VERSION")).map_err(stringify_error)?;
        writeln!(out, "  - Expanded REPL slash commands with status, cost, fast, memory, config, review, auth, and diagnostics")
            .map_err(stringify_error)?;
        writeln!(out, "  - Added custom slash command discovery from project/user `.claude/commands` and plugin command folders")
            .map_err(stringify_error)?;
        writeln!(out, "  - Added `! shell` execution with captured output stored back into session context")
            .map_err(stringify_error)?;
        return Ok(ChatLoopControl::Continue);
    }

    if name == "terminal-setup" {
        writeln!(out, "Terminal Setup").map_err(stringify_error)?;
        if cfg!(windows) {
            writeln!(out, "  Shell            PowerShell").map_err(stringify_error)?;
            writeln!(out, "  Multi-line input Current REPL still submits on Enter; paste multi-line blocks directly when needed")
                .map_err(stringify_error)?;
            writeln!(out, "  Shell escape     Use `! <command>` to run git/npm/python commands inline")
                .map_err(stringify_error)?;
        } else {
            writeln!(out, "  Shell            POSIX shell").map_err(stringify_error)?;
            writeln!(out, "  Shell escape     Use `! <command>` to run commands inline")
                .map_err(stringify_error)?;
        }
        return Ok(ChatLoopControl::Continue);
    }

    if name == "plugin" {
        writeln!(out, "{}", render_plugin_overview(&state.project_context.cwd)?)
            .map_err(stringify_error)?;
        return Ok(ChatLoopControl::Continue);
    }

    if name == "skill" {
        writeln!(out, "{}", render_skill_overview()).map_err(stringify_error)?;
        return Ok(ChatLoopControl::Continue);
    }

    if name == "mcp" {
        writeln!(out, "{}", render_mcp_overview()).map_err(stringify_error)?;
        return Ok(ChatLoopControl::Continue);
    }

    if !matches!(
        name,
        "help" | "exit" | "quit" | "clear" | "session" | "model" | "tools" | "context"
            | "compact" | "init"
    ) {
        let templates = discover_slash_command_templates(&state.project_context.cwd)?;
        if let Some(template) = templates.into_iter().find(|template| template.name == name) {
            let prompt = render_custom_command_prompt(&template, args, &state.project_context.cwd);
            let result = run_one_shot_prompt(
                state,
                &prompt,
                runtime,
                None,
                Some("Execute the slash command instructions faithfully. Keep the output concise and actionable."),
            )?;
            writeln!(out, "{}", result.response_text).map_err(stringify_error)?;
            record_session_usage(
                state,
                result.input_tokens,
                result.output_tokens,
                result.tool_calls,
                result.request_id.clone(),
            );
            append_chat_exchange(state, input.to_string(), result.response_text)?;
            return Ok(ChatLoopControl::Continue);
        }
    }

    match name {
        "help" => {
            write!(out, "{}", render_builtin_slash_help()).map_err(stringify_error)?;
            Ok(ChatLoopControl::Continue)
        }
        "exit" | "quit" => Ok(ChatLoopControl::Exit),
        "clear" => {
            state.messages.clear();
            state.usage = ChatUsageTotals::default();
            state.last_request_id = None;
            save_chat_session(state)?;
            writeln!(out, "Conversation cleared.").map_err(stringify_error)?;
            Ok(ChatLoopControl::Continue)
        }
        // A7: Enhanced /session output with message breakdown and model info
        "session" => {
            let user_count = state.messages.iter().filter(|m| m.role == "user").count();
            let asst_count = state.messages.iter().filter(|m| m.role == "assistant").count();
            writeln!(out, "Session:  {}", state.session_path.display()).map_err(stringify_error)?;
            writeln!(
                out,
                "Messages: {} total ({} user, {} assistant)",
                state.messages.len(),
                user_count,
                asst_count
            )
            .map_err(stringify_error)?;
            writeln!(
                out,
                "Model:    {} \u{2192} {}",
                state.model,
                resolve_model_alias(&state.model)
            )
            .map_err(stringify_error)?;
            writeln!(
                out,
                "Usage:    in={} out={} turns={} tools={}",
                format_token_count(state.usage.input_tokens),
                format_token_count(state.usage.output_tokens),
                state.usage.turns,
                state.usage.tool_calls
            )
            .map_err(stringify_error)?;
            writeln!(
                out,
                "Tools:    {}",
                if state.use_tools { "enabled" } else { "disabled" }
            )
            .map_err(stringify_error)?;
            writeln!(out, "Project:  {}", state.project_context.cwd.display())
                .map_err(stringify_error)?;
            writeln!(out, "Context:  {} file(s) loaded", state.project_context.files.len())
                .map_err(stringify_error)?;
            Ok(ChatLoopControl::Continue)
        }
        "model" => {
            match parts.next() {
                Some("list") => {
                    let settings = load_stats_code_settings(&stats_code_settings_path())?;
                    writeln!(
                        out,
                        "Saved models: {}",
                        if settings.saved_models.is_empty() {
                            "<none>".to_string()
                        } else {
                            settings.saved_models.join(", ")
                        }
                    )
                    .map_err(stringify_error)?;
                    writeln!(
                        out,
                        "Default model: {}",
                        settings.default_model.as_deref().unwrap_or("<none>")
                    )
                    .map_err(stringify_error)?;
                }
                Some("save") => {
                    let model = parts.next().unwrap_or(&state.model).to_string();
                    let result = handle_config_add_model(&ConfigModelArgs {
                        model: model.clone(),
                    })?;
                    writeln!(out, "{}", result.message).map_err(stringify_error)?;
                }
                Some("default") => {
                    let model = parts
                        .next().map_or_else(|| state.model.clone(), str::to_string);
                    let result = handle_config_default_model(&ConfigModelArgs {
                        model: model.clone(),
                    })?;
                    state.model = model;
                    save_chat_session(state)?;
                    writeln!(out, "{}", result.message).map_err(stringify_error)?;
                }
                Some("remove") => {
                    let Some(model) = parts.next() else {
                        return Err("Usage: /model remove <model-name>".to_string());
                    };
                    let result = handle_config_remove_model(&ConfigModelArgs {
                        model: model.to_string(),
                    })?;
                    writeln!(out, "{}", result.message).map_err(stringify_error)?;
                }
                Some(next_model) => {
                    state.model = next_model.to_string();
                    save_chat_session(state)?;
                    writeln!(
                        out,
                        "Model set to {} -> {}",
                        state.model,
                        resolve_model_alias(&state.model)
                    )
                    .map_err(stringify_error)?;
                }
                None => {
                    let settings = load_stats_code_settings(&stats_code_settings_path())?;
                    writeln!(
                        out,
                        "Model: {} -> {}",
                        state.model,
                        resolve_model_alias(&state.model)
                    )
                    .map_err(stringify_error)?;
                    writeln!(
                        out,
                        "Default model: {}",
                        settings.default_model.as_deref().unwrap_or("<none>")
                    )
                    .map_err(stringify_error)?;
                }
            }
            Ok(ChatLoopControl::Continue)
        }
        "tools" => {
            if let Some(mode) = parts.next() {
                match mode {
                    "on" => state.use_tools = true,
                    "off" => state.use_tools = false,
                    _ => {
                        return Err(
                            "Use `/tools on` or `/tools off` to change tool-calling mode."
                                .to_string(),
                        )
                    }
                }
                save_chat_session(state)?;
            }
            writeln!(
                out,
                "Stats tools are {}.",
                if state.use_tools {
                    "enabled"
                } else {
                    "disabled"
                }
            )
            .map_err(stringify_error)?;
            Ok(ChatLoopControl::Continue)
        }
        "context" => {
            match parts.next() {
                Some("reload") => {
                    state.project_context =
                        collect_project_context(&env::current_dir().map_err(stringify_error)?)?;
                    save_chat_session(state)?;
                    writeln!(
                        out,
                        "Context reloaded: {}",
                        format_project_context_summary(&state.project_context)
                    )
                    .map_err(stringify_error)?;
                }
                Some(other) => {
                    return Err(format!(
                        "Unknown `/context` action `{other}`. Use `/context` or `/context reload`."
                    ));
                }
                None => {
                    writeln!(
                        out,
                        "Context: {}",
                        format_project_context_summary(&state.project_context)
                    )
                    .map_err(stringify_error)?;
                    for file in &state.project_context.files {
                        writeln!(out, "  - {}", file.label).map_err(stringify_error)?;
                    }
                }
            }
            Ok(ChatLoopControl::Continue)
        }
        // B1: /compact 闁?AI-powered history compression
        "compact" => {
            if state.messages.is_empty() {
                writeln!(out, "No messages to compact.").map_err(stringify_error)?;
                return Ok(ChatLoopControl::Continue);
            }
            let custom_instructions = parts.collect::<Vec<_>>().join(" ");
            writeln!(out, "\u{29bf} Compacting conversation history...").map_err(stringify_error)?;
            out.flush().map_err(stringify_error)?;

            let history_text = state
                .messages
                .iter()
                .map(|m| {
                    let role = if m.role == "user" { "User" } else { "Assistant" };
                    let text = m
                        .content
                        .iter()
                        .filter_map(|block| {
                            if let api::InputContentBlock::Text { text } = block {
                                Some(text.as_str())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    format!("{role}: {text}")
                })
                .collect::<Vec<_>>()
                .join("\n");

            let compact_prompt = if custom_instructions.trim().is_empty() {
                format!(
                    "Summarize this conversation concisely. Preserve: key findings, \
                     data file paths, analysis decisions, any statistical results. \
                     Be specific and factual.\n\n---\n{history_text}"
                )
            } else {
                format!(
                    "Summarize this conversation. Focus: {custom_instructions}. \
                     Preserve: key findings, data file paths, analysis decisions.\n\n---\n{history_text}"
                )
            };

            let resolved_model = resolve_model_alias(&state.model);
            let provider_kind = detect_provider_kind(&resolved_model);
            let client = prepare_ai_provider(provider_kind, &resolved_model)
                .map_err(|e| format!("Failed to init client for compact: {e}"))?
                .client;
            let compact_request = api::MessageRequest {
                model: resolved_model.clone(),
                max_tokens: 1024,
                messages: vec![api::InputMessage::user_text(compact_prompt)],
                system: Some(
                    "You are a concise summarizer. Output only the summary, no preamble."
                        .to_string(),
                ),
                tools: None,
                tool_choice: None,
                stream: false,
            };
            let compact_response = runtime
                .block_on(async {
                    tokio::time::timeout(
                        std::time::Duration::from_secs(30),
                        client.send_message(&compact_request),
                    )
                    .await
                })
                .map_err(|e| format!("Compact request failed: {e}"))?
                .map_err(|e| format!("Compact request timed out after 30s: {e}"))?;

            let summary = extract_response_text(&compact_response.content);
            if summary.trim().is_empty() {
                writeln!(out, "Compaction returned empty summary; keeping original history.")
                    .map_err(stringify_error)?;
                return Ok(ChatLoopControl::Continue);
            }

            let original_count = state.messages.len();
            state.messages = vec![api::InputMessage::user_text(format!(
                "[Summary of previous conversation]\n{summary}"
            ))];
            record_session_usage(
                state,
                compact_response.usage.input_tokens,
                compact_response.usage.output_tokens,
                0,
                compact_response.request_id.clone(),
            );
            save_chat_session(state)?;
            writeln!(
                out,
                "\u{2713} Compacted: {original_count} messages \u{2192} 1 summary"
            )
            .map_err(stringify_error)?;
            Ok(ChatLoopControl::Continue)
        }
        // B2: /init 闁?generate STATS.md project config file
        "init" => {
            let cwd = env::current_dir().map_err(stringify_error)?;
            writeln!(out, "\u{29bf} Scanning {} for project files...", cwd.display())
                .map_err(stringify_error)?;
            out.flush().map_err(stringify_error)?;

            let mut data_files: Vec<String> = Vec::new();
            let mut config_files: Vec<String> = Vec::new();
            if let Ok(entries) = fs::read_dir(&cwd) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }
                    let file_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    match ext.as_str() {
                        "csv" | "xlsx" | "xls" | "parquet" | "xpt" => {
                            data_files.push(file_name);
                        }
                        "yaml" | "yml" => {
                            config_files.push(file_name);
                        }
                        _ if file_name == "README.md" || file_name == "AGENTS.md" => {
                            config_files.push(file_name);
                        }
                        _ => {}
                    }
                }
            }
            data_files.sort();
            config_files.sort();

            let mut stats_md = String::new();
            stats_md.push_str("# STATS.md\n\n");
            stats_md.push_str(
                "This file provides project context to Stats Code for this working directory.\n\n",
            );
            stats_md.push_str(&format!(
                "## Project\n\n- **Directory**: `{}`\n",
                cwd.display()
            ));
            stats_md.push_str(&format!(
                "- **Stats Code version**: {}\n\n",
                env!("CARGO_PKG_VERSION")
            ));

            if !data_files.is_empty() {
                stats_md.push_str("## Data Files\n\n");
                for f in &data_files {
                    stats_md.push_str(&format!("- `{f}`\n"));
                }
                stats_md.push('\n');
            }

            if !config_files.is_empty() {
                stats_md.push_str("## Config / Spec Files\n\n");
                for f in &config_files {
                    stats_md.push_str(&format!("- `{f}`\n"));
                }
                stats_md.push('\n');
            }

            stats_md.push_str("## Common Commands\n\n");
            if let Some(first_csv) = data_files.iter().find(|f| f.ends_with(".csv")) {
                stats_md.push_str(&format!(
                    "```sh\n# Inspect dataset\nstats-code inspect {first_csv}\n\n"
                ));
                stats_md.push_str(&format!(
                    "# Table 1 (replace GROUP_COL with your grouping variable)\n\
                     stats-code tableone --data {first_csv} --by GROUP_COL\n```\n\n"
                ));
            } else {
                stats_md.push_str(
                    "```sh\n# Inspect dataset\nstats-code inspect <your-data.csv>\n\n\
                     # Table 1\nstats-code tableone --data <your-data.csv> --by GROUP_COL\n```\n\n",
                );
            }
            if let Some(yaml) = config_files
                .iter()
                .find(|f| f.ends_with(".yaml") || f.ends_with(".yml"))
            {
                stats_md.push_str(&format!(
                    "```sh\n# Build full report from analysis spec\nstats-code report build {yaml}\n```\n\n"
                ));
            }

            stats_md.push_str("## Notes\n\n");
            stats_md
                .push_str("- Edit this file to add study context, data dictionary, or analysis notes.\n");
            stats_md.push_str(
                "- Stats Code reads STATS.md automatically when a chat session starts here.\n",
            );

            let stats_md_path = cwd.join("STATS.md");
            fs::write(&stats_md_path, &stats_md).map_err(stringify_error)?;
            state.project_context = collect_project_context(&cwd)?;
            save_chat_session(state)?;

            writeln!(out, "\u{2713} Created {}", stats_md_path.display())
                .map_err(stringify_error)?;
            writeln!(
                out,
                "  {} data file(s) found: {}",
                data_files.len(),
                if data_files.is_empty() {
                    "<none>".to_string()
                } else {
                    data_files.join(", ")
                }
            )
            .map_err(stringify_error)?;
            writeln!(out, "  Project context reloaded.").map_err(stringify_error)?;
            Ok(ChatLoopControl::Continue)
        }
        _ => {
            writeln!(out, "Unknown slash command: /{name}. Type /help.")
                .map_err(stringify_error)?;
            Ok(ChatLoopControl::Continue)
        }
    }
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
    let credential_source = prepared.credential_source.clone();
    let client = prepared.client;
    let mut messages = state.messages.clone();
    messages.push(InputMessage::user_text(user_input.to_string()));

    let max_tokens = if state.fast_mode {
        state.max_tokens
            .unwrap_or_else(|| max_tokens_for_model(&resolved_model).min(1024))
            .min(768)
    } else {
        state.max_tokens
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
                resolved_model,
                credential_source,
                input_tokens,
                output_tokens,
                total_tokens: input_tokens.saturating_add(output_tokens),
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
        fs::create_dir_all(project_root.join(".claude").join("commands")).expect("project commands");
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
            project_root.join(".claude").join("commands").join("project.md"),
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
        assert!(commands.iter().any(|command| command.source.contains("project .claude/commands")));
        assert!(commands.iter().any(|command| command.source.contains("user ~/.claude/commands")));
        assert!(commands.iter().any(|command| command.source.contains("project-plugin:demo")));

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
