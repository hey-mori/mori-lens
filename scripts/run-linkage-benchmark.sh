#!/usr/bin/env bash
#
# Run the 5-run linkage quality benchmark and report aggregate statistics.
#
# This script wraps the Rust ignored test that performs 5 independent runs
# against a live Ollama instance. It captures the output and reports whether
# the pre-committed gates are met.
#
# Prerequisites:
#   - Ollama running locally (ollama serve)
#   - qwen2.5:7b model pulled
#   - translategemma model pulled
#
# Usage:
#   scripts/run-linkage-benchmark.sh
#
# Exit codes:
#   0: all gates passed
#   1: one or more gates failed
#   2: Ollama or model not available
set -euo pipefail

cd "$(dirname "$0")/.."

# Check Ollama availability
if ! command -v ollama &>/dev/null; then
  echo "ERROR: ollama not found in PATH"
  exit 2
fi

if ! curl -s http://localhost:11434/api/tags &>/dev/null; then
  echo "ERROR: Ollama API not reachable at http://localhost:11434"
  echo "Try: ollama serve"
  exit 2
fi

# Check required models
MISSING_MODELS=0
for model in "qwen2.5:7b" "translategemma"; do
  if ! ollama list 2>/dev/null | grep -q "$model"; then
    echo "ERROR: model '$model' not found"
    echo "Try: ollama pull $model"
    MISSING_MODELS=1
  fi
done
if [[ "$MISSING_MODELS" == "1" ]]; then
  exit 2
fi

echo "Running 5-run linkage quality benchmark..."
echo ""

# Run the benchmark test (it does 5 runs internally)
OUTPUT=$(cargo test -p lens-translate -- --ignored --nocapture --test-threads=1 linkage_quality_benchmark 2>&1)
EXIT_CODE=$?

echo "$OUTPUT"
echo ""

if [[ $EXIT_CODE -ne 0 ]]; then
  echo "ERROR: benchmark test failed (exit code $EXIT_CODE)"
  exit 1
fi

# Check for gate results in output
if echo "$OUTPUT" | grep -q "ALL GATES PASSED"; then
  echo "RESULT: ALL GATES PASSED"
  exit 0
elif echo "$OUTPUT" | grep -q "GATE FAILED"; then
  echo "RESULT: ONE OR MORE GATES FAILED"
  exit 1
else
  echo "WARNING: could not determine gate status from output"
  echo "Review the output above manually"
  exit 1
fi
