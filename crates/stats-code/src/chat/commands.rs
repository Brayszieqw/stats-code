// ---------------------------------------------------------------------------
// Built-in and custom slash command dispatch for the chat REPL.
// ---------------------------------------------------------------------------

use std::fmt::Write as _;
use std::io::Write;

#[allow(clippy::wildcard_imports)]
use super::*;

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
        writeln!(
            out,
            "  Model             {} -> {}",
            state.model, resolved_model
        )
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
            if state.fast_mode {
                "enabled"
            } else {
                "disabled"
            }
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
            if state.vim_mode {
                "enabled"
            } else {
                "disabled"
            }
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
                writeln!(
                    out,
                    "{}",
                    render_config_text(&handle_config_show().map_err(stringify_error)?)
                )
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
                })
                .map_err(stringify_error)?;
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
                                model,
                                pricing.input_per_million_usd,
                                pricing.output_per_million_usd
                            )
                            .map_err(stringify_error)?;
                        }
                    }
                } else {
                    let model = maybe_model.unwrap_or_default();
                    let input_usd = maybe_input
                        .ok_or_else(|| {
                            "Usage: /config pricing <model> <input_usd_per_1m> <output_usd_per_1m>"
                                .to_string()
                        })?
                        .parse::<f64>()
                        .map_err(|_| "Input price must be a number.".to_string())?;
                    let output_usd = maybe_output
                        .ok_or_else(|| {
                            "Usage: /config pricing <model> <input_usd_per_1m> <output_usd_per_1m>"
                                .to_string()
                        })?
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
            &[
                "diff",
                "--minimal",
                "--no-ext-diff",
                "--no-color",
                "--unified=1",
            ],
            cwd,
        )?;
        let review_material = format!(
            "Git status:\n{}\n\nGit diff:\n{}\n\nStatus stderr:\n{}\n\nDiff stderr:\n{}",
            if status_stdout.trim().is_empty() {
                "<clean>"
            } else {
                status_stdout.trim()
            },
            if diff_stdout.trim().is_empty() {
                "<no unstaged diff>"
            } else {
                diff_stdout.trim()
            },
            if status_stderr.trim().is_empty() {
                "<none>"
            } else {
                status_stderr.trim()
            },
            if diff_stderr.trim().is_empty() {
                "<none>"
            } else {
                diff_stderr.trim()
            },
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
        let (stdout, stderr, exit_code) = run_process_capture(
            "gh",
            &["pr", "view", "--comments"],
            &state.project_context.cwd,
        )?;
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
        })
        .map_err(stringify_error)?;
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
            if removed {
                "Removed saved credentials for"
            } else {
                "No saved credentials for"
            },
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
        writeln!(out, "  - Added custom slash command discovery from project/user `.stats-code/commands` and plugin command folders")
            .map_err(stringify_error)?;
        writeln!(
            out,
            "  - Added `! shell` execution with captured output stored back into session context"
        )
        .map_err(stringify_error)?;
        return Ok(ChatLoopControl::Continue);
    }

    if name == "terminal-setup" {
        writeln!(out, "Terminal Setup").map_err(stringify_error)?;
        if cfg!(windows) {
            writeln!(out, "  Shell            PowerShell").map_err(stringify_error)?;
            writeln!(out, "  Multi-line input Current REPL still submits on Enter; paste multi-line blocks directly when needed")
                .map_err(stringify_error)?;
            writeln!(
                out,
                "  Shell escape     Use `! <command>` to run git/npm/python commands inline"
            )
            .map_err(stringify_error)?;
        } else {
            writeln!(out, "  Shell            POSIX shell").map_err(stringify_error)?;
            writeln!(
                out,
                "  Shell escape     Use `! <command>` to run commands inline"
            )
            .map_err(stringify_error)?;
        }
        return Ok(ChatLoopControl::Continue);
    }

    if name == "plugin" {
        writeln!(
            out,
            "{}",
            render_plugin_overview(&state.project_context.cwd)?
        )
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
        "help"
            | "exit"
            | "quit"
            | "clear"
            | "session"
            | "model"
            | "tools"
            | "context"
            | "compact"
            | "init"
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
        "session" => {
            let user_count = state.messages.iter().filter(|m| m.role == "user").count();
            let asst_count = state
                .messages
                .iter()
                .filter(|m| m.role == "assistant")
                .count();
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
                if state.use_tools {
                    "enabled"
                } else {
                    "disabled"
                }
            )
            .map_err(stringify_error)?;
            writeln!(out, "Project:  {}", state.project_context.cwd.display())
                .map_err(stringify_error)?;
            writeln!(
                out,
                "Context:  {} file(s) loaded",
                state.project_context.files.len()
            )
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
                    })
                    .map_err(stringify_error)?;
                    writeln!(out, "{}", result.message).map_err(stringify_error)?;
                }
                Some("default") => {
                    let model = parts
                        .next()
                        .map_or_else(|| state.model.clone(), str::to_string);
                    let result = handle_config_default_model(&ConfigModelArgs {
                        model: model.clone(),
                    })
                    .map_err(stringify_error)?;
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
                    })
                    .map_err(stringify_error)?;
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
        "compact" => {
            if state.messages.is_empty() {
                writeln!(out, "No messages to compact.").map_err(stringify_error)?;
                return Ok(ChatLoopControl::Continue);
            }
            let custom_instructions = parts.collect::<Vec<_>>().join(" ");
            writeln!(out, "\u{29bf} Compacting conversation history...")
                .map_err(stringify_error)?;
            out.flush().map_err(stringify_error)?;

            let history_text = state
                .messages
                .iter()
                .map(|m| {
                    let role = if m.role == "user" {
                        "User"
                    } else {
                        "Assistant"
                    };
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
                writeln!(
                    out,
                    "Compaction returned empty summary; keeping original history."
                )
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
        "init" => {
            let cwd = env::current_dir().map_err(stringify_error)?;
            writeln!(
                out,
                "\u{29bf} Scanning {} for project files...",
                cwd.display()
            )
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
            let _ = write!(
                stats_md,
                "## Project\n\n- **Directory**: `{}`\n",
                cwd.display()
            );
            let _ = write!(
                stats_md,
                "- **Stats Code version**: {}\n\n",
                env!("CARGO_PKG_VERSION")
            );

            if !data_files.is_empty() {
                stats_md.push_str("## Data Files\n\n");
                for f in &data_files {
                    let _ = writeln!(stats_md, "- `{f}`");
                }
                stats_md.push('\n');
            }

            if !config_files.is_empty() {
                stats_md.push_str("## Config / Spec Files\n\n");
                for f in &config_files {
                    let _ = writeln!(stats_md, "- `{f}`");
                }
                stats_md.push('\n');
            }

            stats_md.push_str("## Common Commands\n\n");
            if let Some(first_csv) = data_files.iter().find(|f| {
                Path::new(f)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
            }) {
                let _ = write!(
                    stats_md,
                    "```sh\n# Inspect dataset\nstats-code inspect {first_csv}\n\n"
                );
                let _ = write!(
                    stats_md,
                    "# Table 1 (replace GROUP_COL with your grouping variable)\n\
                     stats-code tableone --data {first_csv} --by GROUP_COL\n```\n\n"
                );
            } else {
                stats_md.push_str(
                    "```sh\n# Inspect dataset\nstats-code inspect <your-data.csv>\n\n\
                     # Table 1\nstats-code tableone --data <your-data.csv> --by GROUP_COL\n```\n\n",
                );
            }
            if let Some(yaml) = config_files.iter().find(|f| {
                Path::new(f).extension().is_some_and(|ext| {
                    ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml")
                })
            }) {
                let _ = write!(
                    stats_md,
                    "```sh\n# Build full report from analysis spec\nstats-code report build {yaml}\n```\n\n"
                );
            }

            stats_md.push_str("## Notes\n\n");
            stats_md.push_str(
                "- Edit this file to add study context, data dictionary, or analysis notes.\n",
            );
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
