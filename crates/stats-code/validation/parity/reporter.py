"""
parity/reporter.py — Report generation for the Validation Correctness Framework.

Produces:
  - report.json  (machine-readable, consumed by CI)
  - report.md    (human-readable, suitable for documentation / academic citation)

Plus the new (task 11.4) parity reporter that consumes ``ParityRow``:
  - report.json  (machine-readable, consumed by CI / release automation)
  - report.html  (human-readable, attached to GitHub Releases)
"""

from __future__ import annotations

import html
import json
from collections import defaultdict
from pathlib import Path
from typing import Any

from .result import (
    ParityReportHeader,
    ParityRow,
    ParityVerdict,
    RunMetadata,
    SkippedReason,
    Status,
    ValidationResult,
)

SCHEMA_VERSION = "1.0"

# Schema version for the new ParityRow-backed report. Bumped when the JSON
# layout changes in a way that would break downstream tooling. Independent
# of SCHEMA_VERSION above, which is the legacy ValidationResult layout.
PARITY_REPORT_SCHEMA_VERSION = "1.0"


class ReportGenerator:
    """
    Generate JSON and Markdown validation reports from a list of ValidationResult.

    Usage::

        metadata = collect_metadata()
        gen = ReportGenerator(results, metadata)
        gen.write(Path("validation/reports/"))
    """

    def __init__(
        self,
        results: list[ValidationResult],
        metadata: RunMetadata,
    ) -> None:
        self.results = results
        self.metadata = metadata

    # -----------------------------------------------------------------------
    # JSON report
    # -----------------------------------------------------------------------

    def render_json(self) -> str:
        """Return the full JSON report as a string."""
        doc: dict[str, Any] = {
            "schema_version": SCHEMA_VERSION,
            "metadata": self._metadata_dict(),
            "summary": self._summary_dict(),
            "results": [self._result_dict(r) for r in self.results],
        }
        return json.dumps(doc, indent=2, ensure_ascii=False)

    def _metadata_dict(self) -> dict[str, Any]:
        m = self.metadata
        return {
            "generated_at": m.generated_at,
            "stats_code_commit": m.stats_code_commit,
            "stats_code_version": m.stats_code_version,
            "python_version": m.python_version,
            "rscript_version": m.rscript_version,
            "os": m.os,
            "reference_engines": m.reference_engine_versions,
        }

    def _summary_dict(self) -> dict[str, Any]:
        total = len(self.results)
        counts: dict[str, int] = {s.value: 0 for s in Status}
        by_method: dict[str, dict[str, int]] = defaultdict(
            lambda: {s.value: 0 for s in Status}
        )

        for r in self.results:
            counts[r.status.value] += 1
            by_method[r.method][r.status.value] += 1

        # Determine overall status
        has_fail_or_error = counts[Status.FAIL.value] > 0 or counts[Status.ERROR.value] > 0
        has_pass = counts[Status.PASS.value] > 0
        if has_fail_or_error:
            overall = "VALIDATION FAILED"
        elif has_pass:
            overall = "VALIDATED"
        else:
            overall = "NO COMPARISONS"

        return {
            "status": overall,
            "total_comparisons": total,
            "pass": counts[Status.PASS.value],
            "fail": counts[Status.FAIL.value],
            "skip": counts[Status.SKIP.value],
            "error": counts[Status.ERROR.value],
            "by_method": dict(by_method),
        }

    @staticmethod
    def _result_dict(r: ValidationResult) -> dict[str, Any]:
        return {
            "method": r.method,
            "dataset": r.dataset,
            "reference_engine": r.reference_engine,
            "metric": r.metric,
            "tolerance": r.tolerance,
            "status": r.status.value,
            "expected": r.expected,
            "actual": r.actual,
            "difference": r.difference,
            "message": r.message,
            "details": r.details,
        }

    # -----------------------------------------------------------------------
    # Markdown report
    # -----------------------------------------------------------------------

    def render_markdown(self) -> str:
        """Return the full Markdown report as a string."""
        summary = self._summary_dict()
        lines: list[str] = []

        # ── Header ──────────────────────────────────────────────────────────
        status_icon = "✅" if summary["status"] == "VALIDATED" else "❌"
        lines.append("# Stats Code Validation Report")
        lines.append("")
        lines.append(f"**Status: {status_icon} {summary['status']}**")
        lines.append("")
        m = self.metadata
        lines.append(f"Generated: {m.generated_at}")
        lines.append(
            f"Stats Code: commit `{m.stats_code_commit[:12] if m.stats_code_commit != 'unknown' else 'unknown'}`"
        )
        lines.append(
            f"Python: {m.python_version} | R: {m.rscript_version} | OS: {m.os}"
        )
        lines.append("")

        # ── Summary table ───────────────────────────────────────────────────
        lines.append("## Summary")
        lines.append("")
        lines.append("| Method | Pass | Fail | Skip | Error |")
        lines.append("| --- | ---: | ---: | ---: | ---: |")
        for method, counts in sorted(summary["by_method"].items()):
            lines.append(
                f"| {method} "
                f"| {counts.get('pass', 0)} "
                f"| {counts.get('fail', 0)} "
                f"| {counts.get('skip', 0)} "
                f"| {counts.get('error', 0)} |"
            )
        lines.append(
            f"| **Total** "
            f"| **{summary['pass']}** "
            f"| **{summary['fail']}** "
            f"| **{summary['skip']}** "
            f"| **{summary['error']}** |"
        )
        lines.append("")

        # ── Failures ────────────────────────────────────────────────────────
        lines.append("## Failures")
        lines.append("")
        failures = [r for r in self.results if r.status == Status.FAIL]
        if not failures:
            lines.append("_No failures._")
        else:
            lines.append("| Method | Dataset | Reference | Metric | Expected | Actual | Diff | Tolerance |")
            lines.append("| --- | --- | --- | --- | ---: | ---: | ---: | ---: |")
            for r in failures:
                lines.append(
                    f"| {r.method} | {r.dataset} | {r.reference_engine} | {r.metric} "
                    f"| {r.expected} | {r.actual} | {r.difference:.3e} | {r.tolerance:.3e} |"
                )
        lines.append("")

        # ── Errors ──────────────────────────────────────────────────────────
        errors = [r for r in self.results if r.status == Status.ERROR]
        if errors:
            lines.append("## Errors")
            lines.append("")
            for r in errors:
                lines.append(f"- **{r.method}/{r.metric}** ({r.dataset}): {r.message}")
            lines.append("")

        # ── Per-method details ───────────────────────────────────────────────
        lines.append("## Per-method Details")
        lines.append("")
        by_method: dict[str, list[ValidationResult]] = defaultdict(list)
        for r in self.results:
            by_method[r.method].append(r)

        for method in sorted(by_method):
            method_results = by_method[method]
            datasets = sorted({r.dataset for r in method_results})
            engines = sorted({r.reference_engine for r in method_results})
            pass_count = sum(1 for r in method_results if r.status == Status.PASS)
            total_count = len(method_results)

            lines.append(f"### {method}")
            lines.append("")
            lines.append(f"- Datasets: {', '.join(datasets)}")
            lines.append(f"- References: {', '.join(engines)}")
            lines.append(f"- Results: {pass_count}/{total_count} pass")
            lines.append("")
            lines.append("| Dataset | Reference | Metric | Status | Expected | Actual | Diff |")
            lines.append("| --- | --- | --- | --- | ---: | ---: | ---: |")
            for r in method_results:
                diff_str = f"{r.difference:.3e}" if r.difference is not None else "—"
                exp_str = f"{r.expected}" if r.expected is not None else "—"
                act_str = f"{r.actual}" if r.actual is not None else "—"
                lines.append(
                    f"| {r.dataset} | {r.reference_engine} | {r.metric} "
                    f"| {r.status.value} | {exp_str} | {act_str} | {diff_str} |"
                )
            lines.append("")

        # ── Appendix: Tolerance Rationale ───────────────────────────────────
        lines.append("## Appendix: Tolerance Rationale")
        lines.append("")
        lines.append(
            "Tolerance values are defined in `validation/tolerance_config.yaml`. "
            "The rationale for each value is documented in the design document "
            "(`design.md`, section \"容差决策表\")."
        )
        lines.append("")
        lines.append("| Category | Tolerance | Justification |")
        lines.append("| --- | --- | --- |")
        lines.append("| Closed-form (linear, KM, rate) | ≤ 1e-8 | QR/SVD or product-limit; double precision |")
        lines.append("| Iterative (logistic) | ≤ 1e-5 | IRLS convergence + cross-engine diff |")
        lines.append("| Iterative (Cox) | ≤ 1e-4 | Partial likelihood + tie handling |")
        lines.append("| CDF-derived p-values | ≤ 1e-6 | CDF algorithm differences |")
        lines.append("| Math core CDFs | ≤ 1e-10 | Same algorithm family as scipy |")
        lines.append("| Integer / exact | 0.0 | Must match exactly |")
        lines.append("")

        # ── Reference engine versions ────────────────────────────────────────
        lines.append("## Reference Engine Versions")
        lines.append("")
        lines.append("| Engine | Version |")
        lines.append("| --- | --- |")
        for engine, version in sorted(m.reference_engine_versions.items()):
            lines.append(f"| {engine} | {version} |")
        lines.append(f"| Rscript | {m.rscript_version} |")
        lines.append("")

        return "\n".join(lines)

    # -----------------------------------------------------------------------
    # Write to disk
    # -----------------------------------------------------------------------

    def write(self, out_dir: Path) -> None:
        """Write ``report.json`` and ``report.md`` to *out_dir*."""
        out_dir.mkdir(parents=True, exist_ok=True)

        json_path = out_dir / "report.json"
        md_path = out_dir / "report.md"

        json_path.write_text(self.render_json(), encoding="utf-8")
        md_path.write_text(self.render_markdown(), encoding="utf-8")


# ===========================================================================
# ParityReportGenerator (task 11.4 — Requirements 3.1 / 3.2 / 3.3 / 3.5 / 3.6
# / 3.7 / 12.4 / 12.5)
#
# Consumes the new ``ParityRow`` model defined in `parity/result.py` (task
# 11.1) and emits ``report.json`` + ``report.html`` under
# ``crates/stats-code/validation/reports/<run_id>/``.
#
# Coexists with the legacy ``ReportGenerator`` above; nothing here renames
# or alters that surface.
# ===========================================================================


# Numeric formatting (Requirements 3.3, 12.5):
#   ``f"{x:.12e}"`` yields one digit before the decimal point and 12 digits
#   after it — that is 13 significant digits, which clears the ≥12 floor.
#   ``None`` is rendered as the literal string ``n/a``.
NA_LITERAL = "n/a"
SIG_FIGS_FORMAT = "{:.12e}"


def _fmt_numeric(value: float | None) -> str:
    """Format one numeric field for the report.

    Returns the literal string ``n/a`` when ``value`` is None, or a string
    of at least 12 significant digits via ``f"{x:.12e}"`` otherwise.
    """
    if value is None:
        return NA_LITERAL
    return SIG_FIGS_FORMAT.format(value)


def _row_to_dict(row: ParityRow) -> dict[str, Any]:
    """Render one ParityRow into the JSON-serialisable dict that goes in
    ``report.json``.

    All 14 fields of ``ParityRow`` appear in the output. Numeric fields are
    rendered via ``_fmt_numeric`` (≥12 sig digits, or ``n/a`` for None).
    Enums are rendered by ``.value`` so the JSON contract matches the Rust
    snake_case serde rename.
    """
    return {
        "algorithm_id": row.algorithm_id,
        "algorithm_display_name": row.algorithm_display_name,
        "software": row.software,
        "reference_impl": {
            "name": row.reference_impl.name,
            "pkg": row.reference_impl.pkg,
            "version": row.reference_impl.version,
        },
        "case_id": row.case_id,
        "metric": row.metric,
        "stats_engine_value": _fmt_numeric(row.stats_engine_value),
        "reference_value_or_na": _fmt_numeric(row.reference_value_or_na),
        "absolute_difference": _fmt_numeric(row.absolute_difference),
        "relative_difference": _fmt_numeric(row.relative_difference),
        "active_absolute_tolerance": _fmt_numeric(row.active_absolute_tolerance),
        "active_relative_tolerance": _fmt_numeric(row.active_relative_tolerance),
        "verdict": row.verdict.value,
        "skipped_reason": (
            row.skipped_reason.value if row.skipped_reason is not None else None
        ),
    }


def _header_to_dict(header: ParityReportHeader) -> dict[str, Any]:
    """Render the header into a JSON-serialisable dict.

    ``reference_software_versions`` is sorted by key so the JSON output is
    deterministic across dict-insertion orders. ``tolerance_diff`` and
    ``coverage_matrix`` are emitted as-is (the matrix preserves the
    ``none`` marker per Requirement 3.7; the diff list preserves PR-order).
    """
    return {
        "commit_sha": header.commit_sha,
        "run_started_at_utc": header.run_started_at_utc,
        "host_os_family": header.host_os_family,
        "host_os_version": header.host_os_version,
        "reference_software_versions": dict(
            sorted(header.reference_software_versions.items())
        ),
        "coverage_matrix": header.coverage_matrix,
        "tolerance_diff": list(header.tolerance_diff),
    }


def _summary(rows: list[ParityRow]) -> dict[str, int]:
    """Tally pass / fail / skipped / total counts for the HTML summary."""
    counts = {v.value: 0 for v in ParityVerdict}
    for row in rows:
        counts[row.verdict.value] += 1
    counts["total"] = len(rows)
    return counts


# ---------------------------------------------------------------------------
# CSS for report.html — kept as a module-level constant so determinism is
# trivial (no string interpolation, no clock reads).
# ---------------------------------------------------------------------------

_REPORT_CSS = """\
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
       margin: 2rem; color: #1f2328; }
h1, h2 { border-bottom: 1px solid #d0d7de; padding-bottom: 0.3rem; }
table { border-collapse: collapse; width: 100%; margin-bottom: 1.5rem;
        font-size: 0.85rem; }
th, td { border: 1px solid #d0d7de; padding: 0.4rem 0.6rem; text-align: left;
         vertical-align: top; }
th { background: #f6f8fa; }
td.num { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas,
         monospace; text-align: right; }
.verdict-pass    { color: #1a7f37; font-weight: 600; }
.verdict-fail    { color: #cf222e; font-weight: 700; }
.verdict-skipped { color: #9a6700; font-weight: 600; }
.summary { display: flex; gap: 1.5rem; margin: 1rem 0; }
.summary div { padding: 0.5rem 1rem; border: 1px solid #d0d7de;
               border-radius: 6px; background: #f6f8fa; }
.kv { font-family: ui-monospace, monospace; }
details { margin: 1rem 0; }
details > summary { cursor: pointer; font-weight: 600; padding: 0.2rem 0; }
pre { background: #f6f8fa; border: 1px solid #d0d7de; padding: 0.6rem;
      border-radius: 6px; overflow-x: auto; font-size: 0.8rem; }
"""


def _esc(s: Any) -> str:
    """HTML-escape a value rendered into the page text."""
    return html.escape(str(s), quote=True)


# ===========================================================================
# Public class
# ===========================================================================


class ParityReportGenerator:
    """Generate ``report.json`` and ``report.html`` from ``ParityRow`` data.

    Usage::

        gen = ParityReportGenerator(rows, header)
        gen.write(Path("crates/stats-code/validation/reports/<run_id>"))

    Determinism: with structurally identical inputs, ``render_json`` is
    byte-identical across calls and across processes (no clock reads, no
    set iteration, sorted reference_software_versions). ``render_html``
    uses the same input ordering plus a constant CSS block.
    """

    def __init__(
        self,
        rows: list[ParityRow],
        header: ParityReportHeader,
    ) -> None:
        self.rows = rows
        self.header = header

    # -----------------------------------------------------------------------
    # JSON
    # -----------------------------------------------------------------------

    def render_json(self) -> str:
        """Return ``report.json`` as a UTF-8 string with LF line endings."""
        doc: dict[str, Any] = {
            "schema_version": PARITY_REPORT_SCHEMA_VERSION,
            "header": _header_to_dict(self.header),
            "rows": [_row_to_dict(r) for r in self.rows],
        }
        # ensure_ascii=False keeps non-ASCII (e.g. algorithm display names)
        # readable; sort_keys=False because we already build a stable dict
        # ourselves and want human-friendly key order.
        return json.dumps(doc, indent=2, ensure_ascii=False)

    # -----------------------------------------------------------------------
    # HTML
    # -----------------------------------------------------------------------

    def render_html(self) -> str:
        """Return ``report.html`` as a UTF-8 string with LF line endings."""
        h = self.header
        summary = _summary(self.rows)

        parts: list[str] = []
        parts.append("<!DOCTYPE html>")
        parts.append('<html lang="en">')
        parts.append("<head>")
        parts.append('<meta charset="utf-8">')
        parts.append("<title>Stats Code Parity Validation Report</title>")
        parts.append("<style>")
        parts.append(_REPORT_CSS)
        parts.append("</style>")
        parts.append("</head>")
        parts.append("<body>")
        parts.append("<h1>Stats Code Parity Validation Report</h1>")

        # ── Header section ────────────────────────────────────────────────
        parts.append("<h2>Run Header</h2>")
        parts.append("<table>")
        parts.append("<tbody>")
        parts.append(
            f'<tr><th>Commit SHA</th><td class="kv">{_esc(h.commit_sha)}</td></tr>'
        )
        parts.append(
            f"<tr><th>Run started (UTC)</th><td>{_esc(h.run_started_at_utc)}</td></tr>"
        )
        parts.append(
            f"<tr><th>Host OS</th><td>{_esc(h.host_os_family)} "
            f"{_esc(h.host_os_version)}</td></tr>"
        )

        # Reference software versions: render sorted-by-key, same as JSON.
        sw_lines = "".join(
            f"<li><span class='kv'>{_esc(name)}</span>: {_esc(version)}</li>"
            for name, version in sorted(h.reference_software_versions.items())
        )
        if not sw_lines:
            sw_lines = "<li><em>none invoked</em></li>"
        parts.append(
            f"<tr><th>Reference software versions</th>"
            f"<td><ul>{sw_lines}</ul></td></tr>"
        )

        # Tolerance diff: PR-modified entries (Requirement 12.4)
        if h.tolerance_diff:
            diff_rows = "".join(
                "<tr>"
                f"<td>{_esc(entry.get('algorithm', ''))}</td>"
                f"<td>{_esc(entry.get('previous', ''))}</td>"
                f"<td>{_esc(entry.get('new', ''))}</td>"
                f"<td>{_esc(entry.get('pr_id', ''))}</td>"
                "</tr>"
                for entry in h.tolerance_diff
            )
            tolerance_block = (
                "<table>"
                "<thead><tr><th>Algorithm</th><th>Previous</th>"
                "<th>New</th><th>PR</th></tr></thead>"
                f"<tbody>{diff_rows}</tbody></table>"
            )
        else:
            tolerance_block = "<em>no tolerance changes in this PR</em>"
        parts.append(
            f"<tr><th>Tolerance changes (PR diff)</th>"
            f"<td>{tolerance_block}</td></tr>"
        )
        parts.append("</tbody>")
        parts.append("</table>")

        # ── Summary ───────────────────────────────────────────────────────
        parts.append("<h2>Summary</h2>")
        parts.append('<div class="summary">')
        parts.append(f"<div>Total: <strong>{summary['total']}</strong></div>")
        parts.append(
            f"<div class='verdict-pass'>Pass: {summary['pass']}</div>"
        )
        parts.append(
            f"<div class='verdict-fail'>Fail: {summary['fail']}</div>"
        )
        parts.append(
            f"<div class='verdict-skipped'>Skipped: {summary['skipped']}</div>"
        )
        parts.append("</div>")

        # ── Per-row table ─────────────────────────────────────────────────
        parts.append("<h2>Parity Rows</h2>")
        parts.append("<table>")
        parts.append(
            "<thead><tr>"
            "<th>Algorithm</th>"
            "<th>Software</th>"
            "<th>Reference</th>"
            "<th>Case</th>"
            "<th>Metric</th>"
            "<th>Stats Engine</th>"
            "<th>Reference Value</th>"
            "<th>Abs Δ</th>"
            "<th>Rel Δ</th>"
            "<th>Abs Tol</th>"
            "<th>Rel Tol</th>"
            "<th>Verdict</th>"
            "<th>Skipped Reason</th>"
            "</tr></thead>"
        )
        parts.append("<tbody>")
        for row in self.rows:
            ref = row.reference_impl
            ref_label = (
                f"{_esc(ref.name)}"
                + (f" ({_esc(ref.pkg)})" if ref.pkg is not None else "")
                + f" v{_esc(ref.version)}"
            )
            verdict_value = row.verdict.value
            verdict_class = f"verdict-{verdict_value}"
            skipped_reason_str = (
                row.skipped_reason.value if row.skipped_reason is not None else ""
            )
            parts.append("<tr>")
            parts.append(
                f"<td>{_esc(row.algorithm_display_name)} "
                f"<small class='kv'>({_esc(row.algorithm_id)})</small></td>"
            )
            parts.append(f"<td>{_esc(row.software)}</td>")
            parts.append(f"<td>{ref_label}</td>")
            parts.append(f"<td class='kv'>{_esc(row.case_id)}</td>")
            parts.append(f"<td class='kv'>{_esc(row.metric)}</td>")
            parts.append(
                f"<td class='num'>{_esc(_fmt_numeric(row.stats_engine_value))}</td>"
            )
            parts.append(
                f"<td class='num'>{_esc(_fmt_numeric(row.reference_value_or_na))}</td>"
            )
            parts.append(
                f"<td class='num'>{_esc(_fmt_numeric(row.absolute_difference))}</td>"
            )
            parts.append(
                f"<td class='num'>{_esc(_fmt_numeric(row.relative_difference))}</td>"
            )
            parts.append(
                f"<td class='num'>{_esc(_fmt_numeric(row.active_absolute_tolerance))}</td>"
            )
            parts.append(
                f"<td class='num'>{_esc(_fmt_numeric(row.active_relative_tolerance))}</td>"
            )
            parts.append(
                f"<td class='{verdict_class}'>{_esc(verdict_value)}</td>"
            )
            parts.append(f"<td>{_esc(skipped_reason_str)}</td>")
            parts.append("</tr>")
        parts.append("</tbody>")
        parts.append("</table>")

        # ── Embedded coverage matrix (Requirement 3.7) ────────────────────
        parts.append("<h2>Algorithm Coverage Matrix</h2>")
        parts.append("<details>")
        parts.append("<summary>Show embedded coverage matrix JSON</summary>")
        # Pretty-printed, deterministic, escaped because <pre> still HTML-decodes.
        cov_json = json.dumps(h.coverage_matrix, indent=2, ensure_ascii=False)
        parts.append(f"<pre>{_esc(cov_json)}</pre>")
        parts.append("</details>")

        parts.append("</body>")
        parts.append("</html>")

        return "\n".join(parts)

    # -----------------------------------------------------------------------
    # Disk
    # -----------------------------------------------------------------------

    def write(self, out_dir: Path) -> None:
        """Write ``report.json`` and ``report.html`` to *out_dir*.

        Output is UTF-8 with LF line endings (``newline="\\n"``) regardless
        of the host platform, so the report is byte-identical across
        Windows / Linux / macOS runners.
        """
        out_dir.mkdir(parents=True, exist_ok=True)

        json_path = out_dir / "report.json"
        html_path = out_dir / "report.html"

        # ``newline=""`` would forward '\n' to the OS-default; we explicitly
        # write LF so Windows runners produce the same bytes as Linux/macOS.
        json_path.write_bytes(self.render_json().encode("utf-8"))
        html_path.write_bytes(self.render_html().encode("utf-8"))
