"""Smoke test: .github/workflows/ci.yml contains the validation job with required structure."""
from pathlib import Path

import yaml

# Navigate from tests/smoke/ up to workspace root:
# test_ci_workflow.py → smoke/ → tests/ → validation/ → stats-code/ → crates/ → workspace root
WORKSPACE_ROOT = Path(__file__).resolve().parents[5]
CI_YAML = WORKSPACE_ROOT / ".github" / "workflows" / "ci.yml"


def test_ci_yaml_exists():
    assert CI_YAML.exists(), f"CI workflow not found at {CI_YAML}"


def _load_ci() -> dict:
    with open(CI_YAML, encoding="utf-8") as f:
        return yaml.safe_load(f)


def test_validation_job_exists():
    ci = _load_ci()
    jobs = ci.get("jobs", {})
    assert "validation" in jobs, (
        f"No 'validation' job in ci.yml. Found jobs: {list(jobs.keys())}"
    )


def test_validation_job_has_python_setup():
    ci = _load_ci()
    steps = ci["jobs"]["validation"].get("steps", [])
    step_names = [s.get("name", "") + " " + str(s.get("uses", "")) for s in steps]
    has_python = any("setup-python" in s or "python" in s.lower() for s in step_names)
    assert has_python, "validation job missing Python setup step"


def test_validation_job_has_upload_artifact():
    ci = _load_ci()
    steps = ci["jobs"]["validation"].get("steps", [])
    has_upload = any("upload-artifact" in str(s.get("uses", "")) for s in steps)
    assert has_upload, "validation job missing upload-artifact step"


def test_validation_job_has_timeout():
    ci = _load_ci()
    job = ci["jobs"]["validation"]
    timeout = job.get("timeout-minutes")
    assert timeout is not None, "validation job missing timeout-minutes"
    assert int(timeout) <= 10, f"timeout-minutes should be ≤10, got {timeout}"
