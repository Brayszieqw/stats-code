# Stats Code

`Stats Code` is a clean-room epidemiology and preventive medicine statistics CLI scaffold.

The first landed scope is intentionally narrow:

- interactive `chat`
- `init`
- `doctor`
- `plan`
- `inspect`
- `check`
- `tableone`
- `rate`
- `model logistic`
- `model cox`
- `model linear`
- `workflow run`
- `report build`
- `report verify`
- `audit explain`
- `open report`
- `run python`
- `run r`
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
- `init` for creating a synthetic demo project with `analysis.yaml`, demo data, and a quickstart README
- `doctor` for checking local CLI readiness, bundled templates, write permission, optional audit tooling, and provider config presence
- `plan` for previewing the declared deterministic workflow before any statistics or artifacts are produced
- persisted default model and saved model list under the Stats Code config directory
- optional `profile.toml` + `env.json` config loading for OpenCode-style provider and model defaults
- tool-calling chat mode that can invoke local `inspect`, `tableone`, `rate`, `model logistic`, `model cox`, `model linear`, `workflow run`, and `report build`
- `inspect` for CSV with missingness, inferred variable kinds, numeric summaries, and warning flags
- `check` for validating an `analysis.yaml` contract before running statistics
- `tableone` for CSV with grouped baseline summaries for continuous and categorical variables
- `rate` for CSV with grouped person-time rate summaries and approximate 95% Poisson intervals
- `model logistic` for CSV with local deterministic fitting, OR, CI, and basic risk warnings
- `model cox` for CSV with local deterministic fitting, HR, CI, and basic risk warnings
- `model linear` for CSV with local deterministic OLS fitting, coefficients, CI, p-values, and fit metrics
- declared `survey.weight` support for Rust tableone, rate, logistic, Cox, and linear point estimates
- `privacy.small_cell_threshold` suppression in generated report markdown tables
- `workflow run` for one-command deterministic execution of the declared `analysis.yaml` steps plus report generation
- `run python` / `run r` bridge commands for custom scripts that accept Stats Code bridge JSON
- `report build` with audit and manuscript scaffolds
- `report verify` for checking that report evidence points to existing accepted artifacts from the same run identity
- `audit explain` for a human-readable accepted/rejected evidence summary from `audit/evidence-index.json`
- `open report` for opening or printing the generated `report/report.md` path
- `auth set` / `auth doctor` for API-key provider setup
- `ai ask` for minimal provider-backed prompts through OpenAI, Gemini, DeepSeek, and the other configured API providers

## Command Surface

```bash
stats-code
stats-code --model gemini
stats-code --session my-study.json
stats-code chat --no-tools --new-session
stats-code init demo-study
stats-code doctor
stats-code plan analysis.yaml
stats-code config show
stats-code config add-model gemini-2.5-pro
stats-code config default-model gemini-2.5-pro
stats-code inspect data.csv
stats-code check analysis.yaml
stats-code tableone --analysis analysis.yaml --by outcome
stats-code rate --analysis analysis.yaml --event case --person-time fu_pt
stats-code model logistic --analysis analysis.yaml --y disease --x age,bmi,smoke
stats-code model cox --analysis analysis.yaml --time fu_time --event death --x age,bmi,smoke
stats-code model linear --data data.csv --y sbp --x age,bmi,smoke
stats-code workflow run analysis.yaml --out stats-code-artifacts --explore-out scratch-artifacts --no-chat
stats-code run python scripts/custom_summary.py --data data.csv
stats-code auth set openai --api-key sk-...
stats-code auth doctor
stats-code ai ask --model gpt "Summarize the main epidemiology risks in this dataset."
stats-code report build analysis.yaml --out stats-code-artifacts --artifacts stats-code-artifacts
stats-code report verify stats-code-artifacts
stats-code report verify stats-code-artifacts --fail-on-warning
stats-code audit explain stats-code-artifacts
stats-code open report stats-code-artifacts
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

- project `.stats-code/commands/**/*.md`
- user `~/.stats-code/commands/**/*.md`
- plugin command folders such as `.stats-code/plugins/**/.stats-code-plugin/commands/**/*.md`

Use `! <shell command>` inside the chat REPL to run a shell command inline. Stdout/stderr is printed and the result is also stored back into the active session context.

The interactive chat UI now redraws as a lightweight Stats Code terminal page with a top status header, a conversation pane, and a bottom input area.

Typing `/` at the start of a new REPL line now opens the slash-command menu immediately. You can keep typing to filter it, use `Up` and `Down` to move the selection, and press `Tab` to accept the highlighted command. Typing `/` and pressing Enter is still treated as a shortcut for `/help`.

Add `--json` to any command for structured output.

Add `--artifacts-dir path` to persist `command.json`, `result.json`, and `context.json` for each run.

Use `stats-code init demo-study` to create a runnable demo project containing `analysis.yaml`, `data/demo_cohort.csv`, `data/demo_cohort.dictionary.csv`, and a README with the trusted workflow commands. From that project, run `stats-code doctor` to check local readiness before `check` and `workflow run`.

Use `stats-code plan analysis.yaml` to validate the contract and preview the declared step order, output directory, report/audit outputs, strict-policy flags, and survey/privacy boundary notes without running statistics.

Use `stats-code check analysis.yaml` before formal runs. It validates the contract without running statistics: schema version, data readability, declared variables, analysis IDs, required model fields, binary outcomes/events, numeric time/person-time fields, and current survey/privacy enforcement boundaries.

Use `stats-code workflow run analysis.yaml --out stats-code-artifacts --no-chat` for a formal reproducible run. The CLI checks the contract, executes the declared `analyses:` steps in order, writes official step artifacts under the run directory, then calls `report build` against that same directory.

Add `--strict` when a formal run should fail instead of silently carrying unsupported policy boundaries. In strict mode, complex survey variance metadata requires `--allow-unenforced-survey`, de-identification or identifier-handling privacy metadata requires `--allow-unenforced-privacy`, and report verification warnings require `--allow-warnings`. Explicitly allowed survey/privacy exceptions are written into `audit/evidence-index.json` and appended to `report/report.md`.

Weight-only survey metadata is supported by the native Rust engines as observation weights for point estimates. Complex survey variance features such as strata, clusters, replicate weights, cycle handling, and linearized variance still require explicit review and `--allow-unenforced-survey` in strict runs. Report markdown applies `privacy.small_cell_threshold` to suppress positive cells below the threshold, but de-identification and identifier removal still require explicit review and `--allow-unenforced-privacy` in strict runs.

Use exploratory commands with a separate directory, for example `stats-code --artifacts-dir scratch-artifacts tableone --analysis analysis.yaml --by sex`. Exploratory artifacts are tagged separately and are ignored by `report build` unless you opt in with `--include-exploratory`.

Use `stats-code report build ... --artifacts path` to scan previously saved run artifacts and fold observed results into `report.md` and table markdown files. Matching prefers the current dataset fingerprint and falls back to resolved analysis/data paths, so unrelated runs are skipped instead of being merged into the report. `audit/evidence-index.json` records accepted and rejected artifacts, rejection reasons, matched analysis step indexes, and formal/exploratory metadata.

Use `stats-code report verify stats-code-artifacts` after a formal run to check the report evidence chain. It verifies the expected audit/report files, parses `audit/run.json` and `audit/evidence-index.json`, checks analysis/data identity consistency, confirms accepted artifact `result.json` and `context.json` paths still exist, flags unexpected exploratory evidence, and summarizes rejected artifacts.

`report verify` exits with code 1 when verification errors are found. Add `--fail-on-warning` to make warning-only verification exit with code 2 for stricter CI scripts.

Use `stats-code audit explain stats-code-artifacts` when you want a readable accepted/rejected evidence summary after verification. It reads `audit/evidence-index.json`, lists accepted artifacts, rejected artifacts and rejection reasons, and reports any policy exceptions recorded for the run.

Use `stats-code open report stats-code-artifacts` to open the generated markdown report with the operating system. Add `--print-only` to print the path without launching an external viewer.

## Verification And Release

Numeric parity checks can be run from the workspace root:

```bash
python crates/stats-code/scripts/verify_numeric_parity.py
```

The parity script compares linear and logistic models against Python statsmodels, Cox models against lifelines, and uses Rscript/survival when available. If Rscript is missing, the R/survival leg is reported as a skip rather than silently assumed.

Create a local release package with:

```powershell
powershell -ExecutionPolicy Bypass -File crates/stats-code/scripts/package-release.ps1
```

The package script builds the release binary, stages README, quickstart/install notes, example contract/data, a zip archive, and `SHA256SUMS.txt` under `target/stats-code-release/`.

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

The bundled `examples/data/demo_cohort.csv` and dictionary are synthetic demo data for smoke tests and workflow examples, not epidemiologic inference.

Top-level sections:

- `schema_version`: contract version for audit/replay
- `study`: title, design, population
- `study_context`: estimand, exposure, comparator, outcome, time zero, follow-up, censoring, missing-data strategy, clustering, sensitivity analyses, reporting guideline
- `data`: path, format, optional `id_column`, dictionary path, encoding
- `variables`: variable dictionary with `kind`, `roles`, coding, and missing-data metadata
- `survey`: optional survey-weight / strata / cluster metadata recorded for design review; native Rust engines apply weight-only metadata to point estimates, while complex-survey variance still requires explicit review
- `privacy`: optional de-identification and small-cell suppression policy metadata; report markdown applies small-cell suppression, while de-identification and identifier removal still require explicit review
- `analyses`: declared analysis steps, each with a stable `id`
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
- `stats-code-artifacts/tables/model-linear-summary.md`

Per-run artifacts saved via `--artifacts-dir` now include:

- `command.json`
- `result.json`
- `context.json`

`command.json` and `context.json` carry `artifact_schema_version: "1.0"` for new artifacts. `context.json` also carries the artifact id, Stats Code version, resolved analysis path, analysis fingerprint, resolved data path, data fingerprint, artifact role, formal run id, and analysis step index used to decide whether a saved run belongs to the current `report build`.

`stats-code workflow run analysis.yaml --out stats-code-artifacts` writes formal artifacts directly under `stats-code-artifacts` and tags each declared step as `role=declared`. Chat tools expose the same deterministic path through `workflow_run`, so the model can request one workflow instead of manually juggling `inspect`, `tableone`, and `report_build`.

This layout is meant to be the evidence chain that an upper agent layer calls into, not a conversational guess.

## Current Direction

The CLI is intentionally biased toward:

- deterministic local execution
- audit-first outputs
- survey/privacy metadata carried in the analysis contract with explicit review notes before inference
- agent orchestration as a later optional layer, not the source of truth

Survey weight fields now drive observation-weighted point estimates in the native Rust engines, and `privacy.small_cell_threshold` suppresses small positive cells in generated report markdown tables. Complex-survey variance, de-identification, and identifier removal remain explicit review boundaries.
