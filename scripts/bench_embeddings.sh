#!/usr/bin/env bash
set -euo pipefail

# Embedding Benchmark Runner
# Usage: ./scripts/bench_embeddings.sh [--train] [--output DIR]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="${OUTPUT_DIR:-$REPO_ROOT/benchmark_results}"
TRAIN=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --train)
            TRAIN=true
            shift
            ;;
        --output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

mkdir -p "$OUTPUT_DIR"

echo "=== Embedding Benchmark ==="
echo "Output directory: $OUTPUT_DIR"
echo ""

# Phase 1: Build corpus
echo "Phase 1: Building corpus..."
./aosctl embed corpus \
    --docs-dir "$REPO_ROOT/docs" \
    --code-dir "$REPO_ROOT/crates" \
    --output "$OUTPUT_DIR/corpus.json"

# Phase 2: Baseline benchmark
echo ""
echo "Phase 2: Running baseline benchmark..."
./aosctl embed bench \
    --corpus "$OUTPUT_DIR/corpus.json" \
    --queries "$REPO_ROOT/eval/golden_queries.json" \
    --output "$OUTPUT_DIR/baseline_report.json"

# Phase 3: Fine-tune (optional)
if [[ "$TRAIN" == "true" ]]; then
    echo ""
    echo "Phase 3: Training LoRA adapter..."
    ./aosctl embed train \
        --corpus "$OUTPUT_DIR/corpus.json" \
        --pairs "$OUTPUT_DIR/training_pairs.json" \
        --output "$OUTPUT_DIR/adapter/"

    echo ""
    echo "Phase 4: Running fine-tuned benchmark..."
    ./aosctl embed bench \
        --corpus "$OUTPUT_DIR/corpus.json" \
        --queries "$REPO_ROOT/eval/golden_queries.json" \
        --adapter "$OUTPUT_DIR/adapter/" \
        --output "$OUTPUT_DIR/finetuned_report.json"

    echo ""
    echo "Phase 5: Comparing results..."
    ./aosctl embed compare \
        --baseline "$OUTPUT_DIR/baseline_report.json" \
        --finetuned "$OUTPUT_DIR/finetuned_report.json"
fi

echo ""
echo "=== Done ==="
echo "Results saved to: $OUTPUT_DIR/"
