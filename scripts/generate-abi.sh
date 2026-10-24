#!/usr/bin/env bash
# generate-abi.sh — Issue #135, updated for #258
#
# Extracts the contract's XDR spec from the compiled wasm and writes it to
# contract-interface.json alongside a human-readable summary and complete
# Markdown ABI table (contract/abi-table.md) covering every pub fn in lib.rs.
#
# Usage:
#   ./scripts/generate-abi.sh             # builds and generates JSON + Markdown ABI table
#   ./scripts/generate-abi.sh --markdown  # outputs the Markdown ABI table to stdout
#   ./scripts/generate-abi.sh --check     # exits non-zero if output differs from
#                                         # what a fresh generation would produce
#                                         # (used in CI to detect drift)
#
# Prerequisites:
#   - Rust + wasm32v1-none target (see CONTRIBUTING.md)
#   - Python 3 (standard on development environments)
#   - stellar CLI (optional for raw XDR JSON export): https://developers.stellar.org/docs/tools/developer-tools/cli/stellar-cli

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

WASM_PATH="${ROOT_DIR}/contracts/dao/target/wasm32v1-none/release/ourdao_dao.wasm"
OUT_DIR="${ROOT_DIR}/contract"
OUT_JSON="${OUT_DIR}/contract-interface.json"
OUT_SUMMARY="${OUT_DIR}/interface-summary.txt"
OUT_MARKDOWN="${OUT_DIR}/abi-table.md"
CHECK_MODE=false
MARKDOWN_STDOUT=false

PYTHON_BIN="python3"
if ! command -v python3 >/dev/null 2>&1; then
  if command -v python >/dev/null 2>&1; then
    PYTHON_BIN="python"
  fi
fi

for arg in "$@"; do
  case $arg in
    --check) CHECK_MODE=true ;;
    --markdown) MARKDOWN_STDOUT=true ;;
    *) echo "Unknown argument: $arg" >&2; exit 1 ;;
  esac
done

if $MARKDOWN_STDOUT; then
  "$PYTHON_BIN" "${SCRIPT_DIR}/generate_abi_markdown.py" --stdout
  exit 0
fi

echo "Generating Markdown ABI table from contracts/dao/src/lib.rs..."
mkdir -p "$OUT_DIR"
FRESH_MD="$(mktemp)"
"$PYTHON_BIN" "${SCRIPT_DIR}/generate_abi_markdown.py" --stdout > "$FRESH_MD"

if $CHECK_MODE; then
  if [ ! -f "$OUT_MARKDOWN" ]; then
    echo "CI drift check FAILED: $OUT_MARKDOWN does not exist." >&2
    echo "Run ./scripts/generate-abi.sh and commit the result." >&2
    rm -f "$FRESH_MD"
    exit 1
  fi
  if ! diff -q "$OUT_MARKDOWN" "$FRESH_MD" > /dev/null 2>&1; then
    echo "CI drift check FAILED: $OUT_MARKDOWN is stale." >&2
    echo "Diff:" >&2
    diff "$OUT_MARKDOWN" "$FRESH_MD" >&2
    echo "" >&2
    echo "Run ./scripts/generate-abi.sh and commit the result." >&2
    rm -f "$FRESH_MD"
    exit 1
  fi
  echo "CI drift check PASSED: $OUT_MARKDOWN matches a fresh generation."
  rm -f "$FRESH_MD"
else
  cp "$FRESH_MD" "$OUT_MARKDOWN"
  rm -f "$FRESH_MD"
  echo "Generated: $OUT_MARKDOWN"
fi

# Optional wasm inspection if stellar CLI and wasm build are available
if command -v stellar >/dev/null 2>&1; then
  echo "Building contract wasm..."
  cargo build --target wasm32v1-none --release -p ourdao-dao 2>&1 | tail -5

  if [ -f "$WASM_PATH" ]; then
    FRESH_JSON="$(mktemp)"
    stellar contract inspect --wasm "$WASM_PATH" --output json > "$FRESH_JSON"
    if $CHECK_MODE; then
      if [ -f "$OUT_JSON" ] && ! diff -q "$OUT_JSON" "$FRESH_JSON" > /dev/null 2>&1; then
        echo "CI drift check FAILED: $OUT_JSON is stale." >&2
        rm -f "$FRESH_JSON"
        exit 1
      fi
    else
      cp "$FRESH_JSON" "$OUT_JSON"
      stellar contract inspect --wasm "$WASM_PATH" --output xdr-base64-array > "$OUT_SUMMARY" 2>/dev/null || true
      echo "Generated: $OUT_JSON"
      echo "Generated: $OUT_SUMMARY"
    fi
    rm -f "$FRESH_JSON"
  fi
fi

echo "Done."
