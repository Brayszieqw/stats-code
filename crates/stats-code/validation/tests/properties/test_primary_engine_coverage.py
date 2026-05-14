# Feature: validation-correctness, Property 5: Primary Reference Engine Coverage
"""
Property 5: For every supported method, at least one Python-based adapter
is registered in ADAPTERS_FOR.
"""
import pytest

from parity.adapters import ADAPTERS_FOR

SUPPORTED_METHODS = list(ADAPTERS_FOR.keys())


@pytest.mark.parametrize("method", SUPPORTED_METHODS)
def test_at_least_one_python_adapter(method: str) -> None:
    """Property 5: |{a ∈ adapters_for(m) : a.is_python}| ≥ 1."""
    adapters = ADAPTERS_FOR[method]
    python_adapters = [a for a in adapters if a.is_python]
    assert len(python_adapters) >= 1, (
        f"Method '{method}' has no Python-based reference adapter. "
        f"Registered adapters: {[a.name for a in adapters]}"
    )
