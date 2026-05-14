# Feature: validation-correctness, Property 12: Method Coverage Completeness
"""
Property 12: Every method in METHOD_IMPORTERS is registered and has at least
one adapter in ADAPTERS_FOR.
"""
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
import run_validation
from parity.adapters import ADAPTERS_FOR

REGISTERED_METHODS = list(run_validation.METHOD_IMPORTERS.keys())


def test_all_registered_methods_have_adapters() -> None:
    """Property 12: every registered method has at least one adapter."""
    for method in REGISTERED_METHODS:
        adapters = ADAPTERS_FOR.get(method, [])
        assert len(adapters) >= 1, (
            f"Method '{method}' is registered in METHOD_IMPORTERS "
            f"but has no adapters in ADAPTERS_FOR"
        )


def test_all_adapter_methods_are_registered() -> None:
    """Property 12 (converse): every method in ADAPTERS_FOR is registered."""
    for method in ADAPTERS_FOR:
        assert method in run_validation.METHOD_IMPORTERS, (
            f"Method '{method}' has adapters but is not registered in METHOD_IMPORTERS"
        )


@pytest.mark.parametrize("method", REGISTERED_METHODS)
def test_method_module_is_importable(method: str) -> None:
    """Property 12: each registered method's parity module can be imported."""
    try:
        module = run_validation.METHOD_IMPORTERS[method]()
    except ImportError as exc:
        pytest.fail(f"Cannot import parity module for '{method}': {exc}")

    assert hasattr(module, "collect"), f"parity.{method} missing 'collect'"
    assert hasattr(module, "METHOD"), f"parity.{method} missing 'METHOD'"
    assert module.METHOD == method, (
        f"parity.{method}.METHOD = '{module.METHOD}', expected '{method}'"
    )
