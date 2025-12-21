#!/usr/bin/env bash
#
# overlap-report.sh
#
# Given a list of branches/refs, prints:
# - files changed per branch (vs a base ref)
# - pairwise file-overlap intersections
# - files touched by 2+ branches (and by all branches)
#
# Constraints: bash + git only (no awk/sed/jq/etc).

set -euo pipefail

usage() {
  printf '%s\n' \
    'Usage:' \
    '  scripts/overlap-report.sh [--base <ref>] [--pattern <glob>] [--] <branch...>' \
    '' \
    'Options:' \
    '  -b, --base <ref>       Base ref to diff against (default: main/master/origin/main/origin/master)' \
    '  -p, --pattern <glob>   Add branches/refs matching a glob (matched against refs/heads + refs/remotes)' \
    '  -h, --help             Show help' \
    '' \
    'Examples:' \
    '  scripts/overlap-report.sh lane/a lane/b lane/c' \
    '  scripts/overlap-report.sh --base origin/main lane/a lane/b' \
    '  scripts/overlap-report.sh --pattern '\''origin/lane/*'\'' --base main' \
    '' \
    'Notes:' \
    '  - "Changed files" are computed with: git diff --name-only <base>...<branch>' \
    '    (triple-dot uses merge-base, which is usually what you want for branch comparisons).'
}

die() {
  printf "error: %s\n" "$*" >&2
  exit 2
}

ref_exists() {
  git rev-parse --verify --quiet "${1}^{commit}" >/dev/null 2>&1
}

default_base() {
  local candidate
  for candidate in main master origin/main origin/master; do
    if ref_exists "$candidate"; then
      printf "%s" "$candidate"
      return 0
    fi
  done
  return 1
}

add_unique_branch() {
  local branch="$1"
  local existing
  if ((${#BRANCHES[@]} > 0)); then
    for existing in "${BRANCHES[@]}"; do
      [[ "$existing" == "$branch" ]] && return 0
    done
  fi
  BRANCHES+=("$branch")
}

file_index_of() {
  local target="$1"
  FILE_INDEX=-1

  local i
  for ((i = 0; i < ${#ALL_FILES[@]}; i++)); do
    if [[ "${ALL_FILES[i]}" == "$target" ]]; then
      FILE_INDEX=$i
      return 0
    fi
  done
  return 1
}

branch_has_file() {
  local branch_idx="$1"
  local target="$2"

  local line
  while IFS= read -r line; do
    [[ -n "$line" ]] || continue
    [[ "$line" == "$target" ]] && return 0
  done <<< "${BR_FILES_TEXT[branch_idx]:-}"
  return 1
}

main() {
  git rev-parse --is-inside-work-tree >/dev/null 2>&1 || die "not inside a git work tree"
  cd "$(git rev-parse --show-toplevel)" || die "failed to cd to repo root"

  local base=""
  local pattern=""

  BRANCHES=()

  while (($# > 0)); do
    case "$1" in
      -b|--base)
        base="${2:-}"
        [[ -n "$base" ]] || die "--base requires a ref argument"
        shift 2
        ;;
      -p|--pattern)
        pattern="${2:-}"
        [[ -n "$pattern" ]] || die "--pattern requires a glob argument"
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      --)
        shift
        while (($# > 0)); do
          add_unique_branch "$1"
          shift
        done
        break
        ;;
      -*)
        die "unknown option: $1"
        ;;
      *)
        add_unique_branch "$1"
        shift
        ;;
    esac
  done

  if [[ -z "$base" ]]; then
    base="$(default_base)" || die "could not determine default base; pass --base <ref>"
  fi
  ref_exists "$base" || die "base ref not found: $base"

  if [[ -n "$pattern" ]]; then
    local ref
    while IFS= read -r ref; do
      [[ -n "$ref" ]] || continue
      [[ "$ref" == $pattern ]] || continue
      add_unique_branch "$ref"
    done < <(git for-each-ref --format='%(refname:short)' refs/heads refs/remotes)
  fi

  ((${#BRANCHES[@]} > 0)) || die "no branches/refs provided (pass <branch...> and/or --pattern <glob>)"

  local n="${#BRANCHES[@]}"

  printf "Base: %s\n" "$base"
  printf "Branches (%d):\n" "$n"
  local b
  for b in "${BRANCHES[@]}"; do
    printf "  %s\n" "$b"
  done
  printf "\n"

  BR_FILES_TEXT=()
  BR_FILE_COUNTS=()

  ALL_FILES=()
  ALL_COUNTS=()
  ALL_BRANCHES=()
  FILE_INDEX=-1

  local i
  for ((i = 0; i < n; i++)); do
    b="${BRANCHES[i]}"
    ref_exists "$b" || die "branch/ref not found: $b"

    local files
    files=()
    local line
    while IFS= read -r line; do
      [[ -n "$line" ]] || continue
      files+=("$line")
    done < <(git diff --name-only "$base...$b")

    BR_FILE_COUNTS[i]="${#files[@]}"

    local text=""
    local f
    if ((${#files[@]} > 0)); then
      for f in "${files[@]}"; do
        text+="${f}"$'\n'

        if file_index_of "$f"; then
          ALL_COUNTS[FILE_INDEX]="$((ALL_COUNTS[FILE_INDEX] + 1))"
          ALL_BRANCHES[FILE_INDEX]+="${b}"$'\n'
        else
          ALL_FILES+=("$f")
          ALL_COUNTS+=(1)
          ALL_BRANCHES+=("${b}"$'\n')
        fi
      done
    fi
    BR_FILES_TEXT[i]="$text"

    printf "== %s ==\n" "$b"
    printf "Changed files vs %s: %d\n" "$base" "${BR_FILE_COUNTS[i]}"
    if ((${BR_FILE_COUNTS[i]} == 0)); then
      printf "  (no changes)\n\n"
      continue
    fi
    for f in "${files[@]}"; do
      printf "  %s\n" "$f"
    done
    printf "\n"
  done

  if ((n < 2)); then
    printf "Need >=2 branches for overlap intersections.\n"
    return 0
  fi

  printf "== Pairwise overlaps ==\n"
  local any_pair_overlap=0
  local j
  for ((i = 0; i < n; i++)); do
    for ((j = i + 1; j < n; j++)); do
      local b1="${BRANCHES[i]}"
      local b2="${BRANCHES[j]}"

      local count=0
      local overlaps=""

      local file
      while IFS= read -r file; do
        [[ -n "$file" ]] || continue
        if branch_has_file "$j" "$file"; then
          count=$((count + 1))
          overlaps+="${file}"$'\n'
        fi
      done <<< "${BR_FILES_TEXT[i]}"

      printf "%s ∩ %s  (%d files)\n" "$b1" "$b2" "$count"
      if ((count > 0)); then
        any_pair_overlap=1
        while IFS= read -r file; do
          [[ -n "$file" ]] || continue
          printf "  %s\n" "$file"
        done <<< "$overlaps"
      fi
      printf "\n"
    done
  done
  if ((any_pair_overlap == 0)); then
    printf "(no pairwise overlaps)\n\n"
  fi

  printf "== Files changed in >=2 branches ==\n"
  local max=0
  for ((i = 0; i < ${#ALL_COUNTS[@]}; i++)); do
    ((${ALL_COUNTS[i]} > max)) && max="${ALL_COUNTS[i]}"
  done

  local printed=0
  local c
  for ((c = max; c >= 2; c--)); do
    for ((i = 0; i < ${#ALL_FILES[@]}; i++)); do
      if ((${ALL_COUNTS[i]} == c)); then
        local branches_text="${ALL_BRANCHES[i]%$'\n'}"
        branches_text="${branches_text//$'\n'/, }"
        printf "%dx  %s  [%s]\n" "$c" "${ALL_FILES[i]}" "$branches_text"
        printed=1
      fi
    done
  done
  if ((printed == 0)); then
    printf "(none)\n"
  fi
  printf "\n"

  printf "== Files changed in all %d branches ==\n" "$n"
  printed=0
  for ((i = 0; i < ${#ALL_FILES[@]}; i++)); do
    if ((${ALL_COUNTS[i]} == n)); then
      printf "  %s\n" "${ALL_FILES[i]}"
      printed=1
    fi
  done
  if ((printed == 0)); then
    printf "  (none)\n"
  fi
}

main "$@"
