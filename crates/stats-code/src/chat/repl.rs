//! REPL loop implementation for Stats Code interactive chat.

use std::io;

use api::{InputContentBlock, InputMessage};

use crate::helpers::stringify_error;
use crate::input::{LineEditor, ReadOutcome};
use crate::ui::{ChatEntryKind, ChatUi};

use super::commands::handle_chat_command;
use super::display::{build_chat_ui_status, print_ui_output};
use super::discovery::slash_command_completion_candidates;
use super::session::save_chat_session;
use super::{handle_shell_bang, run_chat_turn, ChatLoopControl, ChatSessionState};

/// Main REPL loop implementing the Stats Code interactive chat experience.
///
/// Handles user input, slash commands, shell bangs, and AI model interactions
/// in a continuous loop until the user exits.
pub(crate) fn run_chat_repl(mut state: ChatSessionState) -> Result<(), String> {
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

/// Accumulate token usage and tool call counts for the current session.
pub(crate) fn record_session_usage(
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

/// Append a user/assistant exchange to the session message history and persist.
pub(crate) fn append_chat_exchange(
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
