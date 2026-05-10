// ---------------------------------------------------------------------------
// UI display helpers for the chat REPL.
// ---------------------------------------------------------------------------

use std::io::Write;

use api::resolve_model_alias;

use crate::config::{
    estimate_session_cost_usd, load_stats_code_settings, stats_code_settings_path,
};
use crate::ui::{ChatEntryKind, ChatUi, ChatUiStatus};

use super::ChatSessionState;

pub(crate) fn build_chat_ui_status(state: &ChatSessionState) -> ChatUiStatus {
    let resolved_model = resolve_model_alias(&state.model);
    let estimated_cost_usd = load_stats_code_settings(&stats_code_settings_path())
        .ok()
        .and_then(|settings| {
            estimate_session_cost_usd(&settings.pricing, &resolved_model, &state.usage)
        });

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

pub(crate) fn print_ui_output(
    ui: &ChatUi,
    out: &mut impl Write,
    kind: ChatEntryKind,
    output: &[u8],
) {
    if output.is_empty() {
        return;
    }
    let rendered = String::from_utf8_lossy(output).trim().to_string();
    if !rendered.is_empty() {
        let _ = ui.print_turn(out, kind, &rendered);
    }
}

pub(crate) fn truncate_for_display(content: impl AsRef<str>, max_chars: usize) -> String {
    let content = content.as_ref();
    if max_chars == 0 {
        return String::new();
    }

    let mut result = String::with_capacity(max_chars.min(content.len()));
    for (count, ch) in content.chars().enumerate() {
        if count >= max_chars {
            result.push_str("\n... [truncated]");
            return result;
        }
        result.push(ch);
    }
    result
}

pub(crate) fn format_token_count(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.2}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_for_display;

    #[test]
    fn truncate_for_display_keeps_short_content() {
        assert_eq!(truncate_for_display("hello", 10), "hello");
    }

    #[test]
    fn truncate_for_display_keeps_exact_limit() {
        assert_eq!(truncate_for_display("hello", 5), "hello");
    }

    #[test]
    fn truncate_for_display_truncates_by_chars() {
        assert_eq!(
            truncate_for_display("a\u{00e9}bc", 2),
            "a\u{00e9}\n... [truncated]"
        );
    }

    #[test]
    fn truncate_for_display_handles_zero_limit() {
        assert_eq!(truncate_for_display("hello", 0), "");
    }
}
