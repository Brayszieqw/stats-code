# Stats Code

`Stats Code` is a clean-room epidemiology and preventive medicine statistics CLI scaffold.

The first landed scope is intentionally narrow:

- interactive `chat`
- `inspect`
- `tableone`
- `rate`
- `model logistic`
- `model cox`
- `report build`
- `config show`
- `config default-model`
- `config add-model`
- `config remove-model`
- `auth set`
- `auth doctor`
- `ai ask`

The current implementation fixes the command surface, `analysis.yaml` contract, and artifact layout first.

What is real today:

- no-arg interactive `stats-code` / `stats code` chat mode with model switching, context clearing, and provider-backed replies
- persisted default model and saved model list under the Stats Code config directory
- optional `profile.toml` + `env.json` config loading for OpenCode-style provider and model defaults
- tool-calling chat mode that can invoke local `inspect`, `tableone`, `rate`, `model logistic`, `model cox`, and `report build`
- `inspect` for CSV with missingness, inferred variable kinds, numeric summaries, and warning flags
- `tableone` for CSV with grouped baseline summaries for continuous and categorical variables
- `rate` for CSV with grouped person-time rate summaries and approximate 95% Poisson intervals
- `model logistic` for CSV with local deterministic fitting, OR, CI, and basic risk warnings
- `model cox` for CSV with local deterministic fitting, HR, CI, and basic risk warnings
- `report build` with audit and manuscript scaffolds
- `auth set` / `auth doctor` for API-key provider setup
- `ai ask` for minimal provider-backed prompts through OpenAI, Gemini, DeepSeek, and the other configured API providers

## Command Surface

```bash
stats-code
stats-code --model gemini
stats-code --session my-study.json
stats-code chat --no-tools --new-session
stats-code config show
stats-code config add-model gemini-2.5-pro
stats-code config default-model gemini-2.5-pro
stats-code inspect data.csv
stats-code tableone --analysis analysis.yaml --by outcome
stats-code rate --analysis analysis.yaml --event case --person_time fu_pt
stats-code model logistic --analysis analysis.yaml --y disease --x age,bmi,smoke
stats-code model cox --analysis analysis.yaml --time fu_time --event death --x age,bmi,smoke
stats-code auth set openai --api-key sk-...
stats-code auth doctor
stats-code ai ask --model gpt "Summarize the main epidemiology risks in this dataset."
stats-code report build analysis.yaml --out stats-code-artifacts --artifacts stats-code-artifacts
```

Run `stats-code` with no subcommand to open the interactive agent session. It now:

- auto-saves the conversation to a session file
- auto-resumes the last project session for the current working directory
- injects default project context from files like `AGENTS.md`, `README.md`, and `analysis.yaml`
- uses the persisted default model when no explicit `--model` is given

Useful slash commands there:

- `/help`
- `/status`
- `/cost`
- `/session`
- `/model <alias>`
- `/model list`
- `/model save [name]`
- `/model default [name]`
- `/model remove <name>`
- `/fast [on|off]`
- `/memory [show|add <text>|reload]`
- `/config [show|env|model|pricing]`
- `/review`
- `/pr_comments`
- `/login <provider> <api-key> [base-url]`
- `/logout <provider>`
- `/bug`
- `/release-notes`
- `/vim [on|off]`
- `/terminal-setup`
- `/tools on|off`
- `/context`
- `/context reload`
- `/clear`
- `/exit`

Custom slash commands are also discovered from:

- project `.claude/commands/**/*.md`
- user `~/.claude/commands/**/*.md`
- plugin command folders such as `.claude/plugins/**/.claude-plugin/commands/**/*.md`

Use `! <shell command>` inside the chat REPL to run a shell command inline. Stdout/stderr is printed and the result is also stored back into the active session context.

The interactive chat UI now redraws as a lightweight Claude Code style terminal page with a top status header, a conversation pane, and a bottom input area.

Typing `/` at the start of a new REPL line now opens the slash-command menu immediately. You can keep typing to filter it, use `Up` and `Down` to move the selection, and press `Tab` to accept the highlighted command. Typing `/` and pressing Enter is still treated as a shortcut for `/help`.

Add `--json` to any command for structured output.

Add `--artifacts-dir path` to persist `command.json`, `result.json`, and `context.json` for each run.

Use `stats-code report build ... --artifacts path` to scan previously saved run artifacts and fold observed results into `report.md` and table markdown files. Matching prefers the current dataset fingerprint and falls back to resolved analysis/data paths, so unrelated runs are skipped instead of being merged into the report.

`stats-code auth set <provider> --api-key ...` stores provider credentials in the local Stats Code auth store. `stats-code ai ask ...` and interactive chat load saved credentials automatically when the corresponding process environment variables are not already set.

If you prefer a simpler OpenCode-style config, Stats Code also reads `profile.toml` and `env.json` under the Stats Code config directory. `profile.toml` can carry fields like `model_provider`, `model`, `review_model`, `model_reasoning_effort`, and `[model_providers.OpenAI]`; `env.json` can hold `OPENAI_API_KEY` and similar provider secrets.

`stats-code config ...` stores saved models and the default model in `settings.json` under the Stats Code config directory. This is the part directly inspired by Doge Code's separate config/model management approach.

Supported API-key providers in the auth helper today:

- `openai`
- `gemini`
- `deepseek`
- `dashscope`
- `moonshot`
- `xai`

## analysis.yaml

See [`examples/analysis.example.yaml`](./examples/analysis.example.yaml).

Top-level sections:

- `schema_version`: contract version for audit/replay
- `study`: title, design, population
- `study_context`: estimand, exposure, comparator, outcome, time zero, follow-up, censoring, missing-data strategy, clustering, sensitivity analyses, reporting guideline
- `data`: path, format, optional `id_column`, dictionary path, encoding
- `variables`: variable dictionary with `kind`, `roles`, coding, and missing-data metadata
- `survey`: optional survey-weight / strata / cluster metadata
- `privacy`: optional de-identification and small-cell suppression policy
- `analyses`: declared analysis steps
- `report`: output policy for report generation
- `audit`: output policy for trace files

When you run analysis-driven commands from `analysis.yaml` such as `tableone`, `rate`, `model ...`, or `report build`, Stats Code validates that `study_context` contains the minimum required research metadata before execution. If required fields are missing, the CLI prints a copy-pasteable `study_context:` template inferred from the current analysis spec.

## Artifact Layout

`stats-code report build analysis.yaml --out stats-code-artifacts` writes:

- `stats-code-artifacts/audit/analysis.normalized.json`
- `stats-code-artifacts/audit/analysis_manifest.json`
- `stats-code-artifacts/audit/commands.json`
- `stats-code-artifacts/audit/run.json`
- `stats-code-artifacts/audit/audit-trail.md`
- `stats-code-artifacts/audit/evidence-index.json`
- `stats-code-artifacts/report/methods.md`
- `stats-code-artifacts/report/study-context.md`
- `stats-code-artifacts/report/variables.md`
- `stats-code-artifacts/report/report.md`
- `stats-code-artifacts/report/reporting-checklist.md`
- `stats-code-artifacts/report/assumptions.md`
- `stats-code-artifacts/tables/README.md`

When matching `result.json` artifacts are found, it also writes observed summaries such as:

- `stats-code-artifacts/tables/tableone.md`
- `stats-code-artifacts/tables/rate-summary.md`
- `stats-code-artifacts/tables/model-logistic-summary.md`
- `stats-code-artifacts/tables/model-cox-summary.md`

Per-run artifacts saved via `--artifacts-dir` now include:

- `command.json`
- `result.json`
- `context.json`

`context.json` carries the resolved analysis path, resolved data path, and data fingerprint used to decide whether a saved run belongs to the current `report build`.

This layout is meant to be the evidence chain that an upper agent layer calls into, not a conversational guess.

## Current Direction

The CLI is intentionally biased toward:

- deterministic local execution
- audit-first outputs
- survey/privacy metadata carried in the analysis contract
- agent orchestration as a later optional layer, not the source of truth
