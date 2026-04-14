#!/usr/bin/env python3
"""
Stats Code Bridge Runner — generic dispatcher.

Usage:
    python bridge_runner.py --input params.json

Reads a JSON request, dispatches to the appropriate analysis module,
and prints a JSON response to stdout.
"""
import argparse
import importlib
import json
import math
import sys
import time
import platform


def sanitize_for_json(obj):
    """Recursively replace non-finite floats for JSON compatibility.

    - float('inf')  → 1e99   (sentinel for unbounded upper CI)
    - float('-inf') → -1e99  (sentinel for unbounded lower CI)
    - float('nan')  → None   (missing)
    """
    if isinstance(obj, float):
        if math.isnan(obj):
            return None
        if math.isinf(obj):
            return 1e99 if obj > 0 else -1e99
        return obj
    if isinstance(obj, dict):
        return {k: sanitize_for_json(v) for k, v in obj.items()}
    if isinstance(obj, (list, tuple)):
        return [sanitize_for_json(v) for v in obj]
    return obj


def main():
    parser = argparse.ArgumentParser(description="Stats Code Bridge Runner")
    parser.add_argument("--input", required=True, help="Path to JSON request file")
    args = parser.parse_args()

    with open(args.input, "r", encoding="utf-8") as f:
        request = json.load(f)

    command = request.get("command", "")
    data_path = request.get("data_path", "")
    params = request.get("params", {})

    start = time.time()
    warnings_list = []

    try:
        # Import the command module (e.g. model_logistic -> model_logistic.py)
        mod = importlib.import_module(command)
        result = mod.run(data_path, params)
    except ImportError as e:
        print(json.dumps({
            "status": "error",
            "engine": "python",
            "engine_version": platform.python_version(),
            "result": {},
            "raw_output": f"Failed to import module '{command}': {e}"
        }))
        sys.exit(1)
    except Exception as e:
        print(json.dumps({
            "status": "error",
            "engine": "python",
            "engine_version": platform.python_version(),
            "result": {},
            "raw_output": f"Execution error in '{command}': {e}"
        }))
        sys.exit(1)

    elapsed_ms = int((time.time() - start) * 1000)

    # Collect warnings from the result if any
    if isinstance(result, dict) and "warnings" in result:
        warnings_list.extend(result.get("warnings", []))

    # Detect package info from module if available
    package_name = getattr(mod, "PACKAGE_NAME", None)
    package_version = getattr(mod, "PACKAGE_VERSION", None)

    response = {
        "status": "ok",
        "engine": "python",
        "engine_version": platform.python_version(),
        "package": package_name,
        "package_version": package_version,
        "result": result,
        "diagnostics": {
            "execution_time_ms": elapsed_ms,
            "warnings": warnings_list,
        },
    }

    print(json.dumps(sanitize_for_json(response), ensure_ascii=False))


if __name__ == "__main__":
    main()

