//! Chat dispatch — turn execution, one-shot prompts, and shell integration.

use std::io::Write;
use std::path::Path;
use std::process::Command as ProcessCommand;

use api::{
    detect_provider_kind, max_tokens_for_model, resolve_model_alias, InputMessage, MessageRequest,
    ToolChoice,
};
use colored::Colorize;

use crate::config::{extract_response_text, prepare_ai_provider};
use crate::helpers::stringify_error;

use super::context::build_chat_system_prompt;
use super::display::truncate_for_display;
use super::session::save_chat_session;
use super::tools::{
    assistant_message_from_response, chat_tool_definitions, collect_pending_tool_uses,
    execute_chat_tool, summarize_tool_input_short,
};
use super::{record_session_usage, ChatLoopControl, ChatSessionState, ChatTurnOutput};

// ---------------------------------------------------------------------------
// Shell capture helpers
// ---------------------------------------------------------------------------

pub(crate) fn run_process_capture(
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

pub(crate) fn run_shell_capture(command: &str, cwd: &Path) -> Result<(String, String, i32), String> {
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

// ---------------------------------------------------------------------------
// Shell bang (!command)
// ---------------------------------------------------------------------------

pub(crate) fn handle_shell_bang(
    shell_command: &str,
    state: &mut ChatSessionState,
    out: &mut impl Write,
) -> Result<ChatLoopControl, String> {
    let (stdout, stderr, exit_code) = run_shell_capture(shell_command, &state.project_context.cwd)?;
    let mut rendered = String::new();
    use std::fmt::Write as _;
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

// ---------------------------------------------------------------------------
// One-shot prompt (no conversation history)
// ---------------------------------------------------------------------------

pub(crate) fn run_one_shot_prompt(
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

// ---------------------------------------------------------------------------
// Full chat turn (multi-round tool loop)
// ---------------------------------------------------------------------------

pub(crate) fn run_chat_turn(
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

    // Cap tool-call rounds to prevent infinite loops
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
