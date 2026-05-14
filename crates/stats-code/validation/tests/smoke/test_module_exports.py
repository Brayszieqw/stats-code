"""Smoke test: all parity modules export METHOD, METRICS, collect; import from shared layer."""
import importlib
import inspect
from pathlib import Path

PARITY_DIR = Path(__file__).resolve().parents[2] / "parity"

# Modules that are method modules (not infrastructure)
METHOD_MODULES = [
    "linear", "logistic", "cox", "survival",
    "rate", "power", "math_core", "tableone", "diagnostic_roc",
]


def test_method_modules_export_required_symbols():
    for mod_name in METHOD_MODULES:
        mod = importlib.import_module(f"parity.{mod_name}")
        assert hasattr(mod, "METHOD"), f"parity.{mod_name} missing 'METHOD'"
        assert hasattr(mod, "METRICS"), f"parity.{mod_name} missing 'METRICS'"
        assert hasattr(mod, "collect"), f"parity.{mod_name} missing 'collect'"
        assert callable(mod.collect), f"parity.{mod_name}.collect is not callable"
        assert isinstance(mod.METHOD, str), f"parity.{mod_name}.METHOD must be str"
        assert isinstance(mod.METRICS, list), f"parity.{mod_name}.METRICS must be list"
        assert len(mod.METRICS) > 0, f"parity.{mod_name}.METRICS is empty"


def test_method_modules_import_from_common():
    """Each method module must import from parity.common and parity.adapters."""
    for mod_name in METHOD_MODULES:
        source_path = PARITY_DIR / f"{mod_name}.py"
        assert source_path.exists(), f"parity/{mod_name}.py not found"
        source = source_path.read_text(encoding="utf-8")
        assert "from .common import" in source or "from parity.common import" in source, (
            f"parity/{mod_name}.py does not import from parity.common"
        )
        assert "from .result import" in source or "from parity.result import" in source, (
            f"parity/{mod_name}.py does not import from parity.result"
        )


def test_collect_signature():
    """collect() must accept dataset_path, tol_config, adapters."""
    for mod_name in METHOD_MODULES:
        mod = importlib.import_module(f"parity.{mod_name}")
        sig = inspect.signature(mod.collect)
        params = list(sig.parameters.keys())
        assert "dataset_path" in params, (
            f"parity.{mod_name}.collect missing 'dataset_path' parameter"
        )
        assert "tol_config" in params, (
            f"parity.{mod_name}.collect missing 'tol_config' parameter"
        )
        assert "adapters" in params, (
            f"parity.{mod_name}.collect missing 'adapters' parameter"
        )
