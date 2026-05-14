"""
parity/reporter.py — Report generation for the Validation Correctness Framework.

Produces:
  - report.json  (machine-readable, consumed by CI)
  - report.md    (human-readable, suitable for documentation / academic citation)
"""

from __future__ import annotations

import json
from collections import defaultdict
from pathlib import Path
from typing import Any

from .result import RunMetadata, Status, ValidationResult

SCHEMA_VERSION = "1.0"


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
