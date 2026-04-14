#!/usr/bin/env python3
"""
Example custom analysis script for stats-code run python.
Reads --input JSON, runs a simple summary, outputs JSON to stdout.
"""
import argparse
import json
import math

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True)
    args = parser.parse_args()

    with open(args.input, "r") as f:
        request = json.load(f)

    data_path = request.get("data_path", "")
    params = request.get("params", {})

    # Simple: count lines and columns
    result = {"status": "ok", "data_path": data_path}
    if data_path:
        try:
            import pandas as pd
            df = pd.read_csv(data_path)
            result["n_rows"] = len(df)
            result["n_cols"] = len(df.columns)
            result["columns"] = list(df.columns)
            result["dtypes"] = {col: str(dt) for col, dt in df.dtypes.items()}
            result["summary"] = {
                col: {
                    "mean": float(df[col].mean()) if df[col].dtype in ["float64", "int64"] else None,
                    "missing": int(df[col].isna().sum()),
                }
                for col in df.columns
            }
        except Exception as e:
            result["error"] = str(e)

    # Sanitize non-finite floats
    def sanitize(obj):
        if isinstance(obj, float):
            if math.isnan(obj):
                return None
            if math.isinf(obj):
                return 1e99 if obj > 0 else -1e99
            return obj
        if isinstance(obj, dict):
            return {k: sanitize(v) for k, v in obj.items()}
        if isinstance(obj, (list, tuple)):
            return [sanitize(v) for v in obj]
        return obj

    print(json.dumps(sanitize(result), ensure_ascii=False))

if __name__ == "__main__":
    main()
