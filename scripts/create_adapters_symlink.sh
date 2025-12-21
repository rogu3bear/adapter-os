#!/usr/bin/env bash
set -euo pipefail

# Create a compatibility symlink from ./adapters to var/adapters (or AOS_ADAPTERS_DIR)
# to avoid breaking tools that still look under ./adapters.
# Usage:
#   AOS_ADAPTERS_DIR=/custom/path/scripts/adapters ./scripts/create_adapters_symlink.sh
# or just run without env to target var/adapters.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="${AOS_ADAPTERS_DIR:-${REPO_ROOT}/var/adapters}"
LINK_PATH="${REPO_ROOT}/adapters"

mkdir -p "${TARGET_DIR}"

if [ -L "${LINK_PATH}" ] || [ -e "${LINK_PATH}" ]; then
  echo "Path ${LINK_PATH} already exists; not modifying."
  exit 0
fi

ln -s "${TARGET_DIR}" "${LINK_PATH}"
echo "Created symlink ${LINK_PATH} -> ${TARGET_DIR}"

