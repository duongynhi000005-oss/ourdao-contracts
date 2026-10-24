#!/usr/bin/env bash
# Verifies that CHANGELOG.md is updated when contracts/ files are modified.
set -euo pipefail

BASE_REF="${1:-origin/main}"
CHANGED_FILES=$(git diff --name-only "$BASE_REF...HEAD" 2>/dev/null || git diff --name-only HEAD~1 2>/dev/null || true)

CONTRACTS_CHANGED=$(echo "$CHANGED_FILES" | grep -E '^contracts/' || true)
CHANGELOG_CHANGED=$(echo "$CHANGED_FILES" | grep -E '^CHANGELOG\.md$' || true)

if [ -n "$CONTRACTS_CHANGED" ] && [ -z "$CHANGELOG_CHANGED" ]; then
    echo "ERROR: PR modifies files under contracts/ but does not update CHANGELOG.md!"
    echo "Please add an entry under ## [Unreleased] in CHANGELOG.md, or ask maintainers to apply the 'skip-changelog' label."
    exit 1
fi

echo "CHANGELOG check passed."
