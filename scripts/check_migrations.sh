#!/usr/bin/env bash

# Migration hygiene guardrail:
# - detects duplicate migration numbers
# - detects gaps (with a small allowlist for historical holes)
# - detects filename collisions (case-insensitive)
#
# Fails fast with a non-zero exit code on violations.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MIGRATIONS_DIR="${ROOT_DIR}/migrations"

if [[ ! -d "$MIGRATIONS_DIR" ]]; then
  echo "❌ migrations directory not found at ${MIGRATIONS_DIR}" >&2
  exit 1
fi

shopt -s nullglob
MIGRATIONS=()
while IFS= read -r FILEPATH; do
  MIGRATIONS+=("$FILEPATH")
done < <(cd "$MIGRATIONS_DIR" && ls [0-9][0-9][0-9][0-9]_*.sql 2>/dev/null | sort)
shopt -u nullglob

if [[ ${#MIGRATIONS[@]} -eq 0 ]]; then
  echo "❌ no migrations found under ${MIGRATIONS_DIR}" >&2
  exit 1
fi

contains_item() {
  local needle="$1"; shift
  for item in "$@"; do
    if [[ "$item" == "$needle" ]]; then
      return 0
    fi
  done
  return 1
}

to_lower() {
  echo "$1" | tr '[:upper:]' '[:lower:]'
}

ERRORS=0
NUMBERS=()
LOWER_BASENAMES=()
BASENAMES=()

for REL_PATH in "${MIGRATIONS[@]}"; do
  BASENAME="${REL_PATH##*/}"
  PREFIX="${BASENAME%%_*}"

  NUMBERS+=("$PREFIX")
  BASENAMES+=("$BASENAME")
  LOWER_BASENAMES+=("$(to_lower "$BASENAME")")
done

# Duplicate numbers (e.g., 0123_foo.sql and 0123_bar.sql)
DUPLICATE_NUMBERS=()
PREV_NUM=""
for NUM in $(printf "%s\n" "${NUMBERS[@]}" | sort); do
  if [[ "$NUM" == "$PREV_NUM" ]] && ! contains_item "$NUM" "${DUPLICATE_NUMBERS[@]}"; then
    DUPLICATE_NUMBERS+=("$NUM")
  fi
  PREV_NUM="$NUM"
done

if [[ ${#DUPLICATE_NUMBERS[@]} -gt 0 ]]; then
  if [[ $ERRORS -eq 0 ]]; then
    echo "❌ Migration hygiene violations detected:"
  fi
  ERRORS=$((ERRORS + 1))
  for DUP in "${DUPLICATE_NUMBERS[@]}"; do
    FILES_FOR_DUP=()
    for BASENAME in "${BASENAMES[@]}"; do
      if [[ "${BASENAME%%_*}" == "$DUP" ]]; then
        FILES_FOR_DUP+=("$BASENAME")
      fi
    done
    printf " - duplicate migration number %s used by: %s\n" "$DUP" "${FILES_FOR_DUP[*]}"
  done
fi

# Historical gaps that are intentionally empty (documented allowlist)
ALLOWED_GAPS=("0136" "0180")

SORTED_NUMBERS=($(printf "%s\n" "${NUMBERS[@]}" | sort -n | uniq))
MISSING=()

for ((i = 1; i < ${#SORTED_NUMBERS[@]}; i++)); do
  PREV=${SORTED_NUMBERS[$((i - 1))]}
  CUR=${SORTED_NUMBERS[$i]}
  EXPECTED=$((10#$PREV + 1))

  while ((EXPECTED < 10#$CUR)); do
    PADDED=$(printf "%04d" "$EXPECTED")
    if ! contains_item "$PADDED" "${ALLOWED_GAPS[@]}"; then
      MISSING+=("$PADDED")
    fi
    EXPECTED=$((EXPECTED + 1))
  done
done

if [[ ${#MISSING[@]} -gt 0 ]]; then
  if [[ $ERRORS -eq 0 ]]; then
    echo "❌ Migration hygiene violations detected:"
  fi
  ERRORS=$((ERRORS + 1))
  printf " - missing migration numbers detected: %s\n" "${MISSING[*]}"
fi

# Filename collisions (case-insensitive)
COLLISIONS=()
PREV_NAME=""
for LOWER_NAME in $(printf "%s\n" "${LOWER_BASENAMES[@]}" | sort); do
  if [[ "$LOWER_NAME" == "$PREV_NAME" ]] && ! contains_item "$LOWER_NAME" "${COLLISIONS[@]}"; then
    COLLISIONS+=("$LOWER_NAME")
  fi
  PREV_NAME="$LOWER_NAME"
done

if [[ ${#COLLISIONS[@]} -gt 0 ]]; then
  if [[ $ERRORS -eq 0 ]]; then
    echo "❌ Migration hygiene violations detected:"
  fi
  ERRORS=$((ERRORS + 1))
  for COLLISION in "${COLLISIONS[@]}"; do
    MATCHING_FILES=()
    for BASENAME in "${BASENAMES[@]}"; do
      if [[ "$(to_lower "$BASENAME")" == "$COLLISION" ]]; then
        MATCHING_FILES+=("$BASENAME")
      fi
    done
    printf " - filename collision detected (case-insensitive): %s\n" "${MATCHING_FILES[*]}"
  done
fi

if [[ $ERRORS -gt 0 ]]; then
  exit 1
fi

echo "✅ Migration hygiene checks passed (${#MIGRATIONS[@]} files scanned)"
