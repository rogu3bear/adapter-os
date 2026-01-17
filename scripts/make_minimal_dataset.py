#!/usr/bin/env python3
import argparse
import json
import os
import random
from pathlib import Path


def build_rows(count: int, seed: int):
    rng = random.Random(seed)
    ops = [
        ("+", lambda a, b: a + b),
        ("-", lambda a, b: a - b),
        ("*", lambda a, b: a * b),
    ]
    rows = []
    for idx in range(count):
        a = rng.randint(1, 20)
        b = rng.randint(1, 20)
        op, fn = rng.choice(ops)
        if op == "-" and b > a:
            a, b = b, a
        result = fn(a, b)
        prompt = f"Compute: {a} {op} {b}."
        response = f"Result: {a} {op} {b} = {result}."
        rows.append(
            {
                "prompt": prompt,
                "response": response,
                "weight": 1.0,
                "metadata": {
                    "row": idx,
                    "seed": seed,
                    "op": op,
                    "source": "minimal_deterministic",
                },
            }
        )
    return rows


def main() -> int:
    script_dir = Path(__file__).resolve().parent
    default_out = script_dir.parent / "var" / "datasets" / "minimal.jsonl"

    parser = argparse.ArgumentParser(
        description="Generate a deterministic minimal JSONL training dataset."
    )
    parser.add_argument(
        "--output",
        default=str(default_out),
        help=f"Output JSONL path (default: {default_out})",
    )
    parser.add_argument(
        "--count",
        type=int,
        default=32,
        help="Number of rows to generate (default: 32)",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=1337,
        help="PRNG seed for deterministic generation (default: 1337)",
    )
    args = parser.parse_args()

    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    rows = build_rows(args.count, args.seed)
    with output_path.open("w", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row, ensure_ascii=True) + "\n")

    print(f"Wrote {len(rows)} rows to {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
