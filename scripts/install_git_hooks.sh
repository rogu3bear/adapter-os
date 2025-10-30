#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")"/.. && pwd)"

git config core.hooksPath .githooks
echo "Git hooks path set to .githooks"
echo "pre-commit duplication scan installed (advisory by default)."
echo "To enforce blocking on clones: export JSCPD_ENFORCE=1"
