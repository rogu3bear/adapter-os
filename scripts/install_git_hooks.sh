#!/usr/bin/env bash
set -euo pipefail

git config core.hooksPath .githooks
echo "Git hooks path set to .githooks"
echo "pre-commit duplication scan installed (advisory by default)."
echo "To enforce blocking on clones: export JSCPD_ENFORCE=1"

if [[ -x ".githooks/pre-commit" ]]; then
  echo "Testing .githooks/pre-commit..."
  .githooks/pre-commit
  echo "Git hooks configured successfully."
else
  echo "Warning: .githooks/pre-commit is missing or not executable." >&2
fi
