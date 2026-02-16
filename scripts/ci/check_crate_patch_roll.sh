#!/usr/bin/env bash
set -euo pipefail

MODE="check"
BASE_REF="${BASE_REF:-origin/main}"

usage() {
  cat <<'EOF'
Usage:
  check_crate_patch_roll.sh [--mode check|autobump] [--base-ref <git-ref>] [--help]

Description:
  Enforces per-crate version bumps based on top-level Rust file churn from:
    git diff --numstat <base-ref>...HEAD

  Counted files:
    crates/<crate>/*.rs  (top-level only, includes build.rs)

  Tiers:
    LOC <= 10                 : no bump required
    11 <= LOC <= 100          : patch bump   x.y.z -> x.y.(z+1)
    101 <= LOC <= 249         : minor bump   x.y.z -> x.(y+1).1
    LOC >= 250                : major bump   x.y.z -> (x+1).0.1

  Scope:
    Only crates with explicit local [package] version = "x.y.z" in crates/*/Cargo.toml
    Crates using version.workspace = true are ignored.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      MODE="${2:-}"
      shift 2
      ;;
    --base-ref)
      BASE_REF="${2:-}"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ "$MODE" != "check" && "$MODE" != "autobump" ]]; then
  echo "--mode must be one of: check, autobump" >&2
  exit 2
fi

if [[ -z "$BASE_REF" ]]; then
  echo "--base-ref must not be empty" >&2
  exit 2
fi

parse_package_version() {
  awk '
    BEGIN { in_pkg = 0 }
    /^\[package\][[:space:]]*$/ { in_pkg = 1; next }
    /^\[/ && in_pkg == 1 { in_pkg = 0 }
    in_pkg == 1 && /^[[:space:]]*version[[:space:]]*=/ {
      if ($0 ~ /workspace[[:space:]]*=[[:space:]]*true/) {
        print "workspace"
        exit 0
      }
      if (match($0, /"[0-9]+\.[0-9]+\.[0-9]+"/)) {
        v = substr($0, RSTART + 1, RLENGTH - 2)
        print v
        exit 0
      }
    }
  '
}

get_current_explicit_version() {
  local crate="$1"
  local manifest="crates/${crate}/Cargo.toml"
  local version

  [[ -f "$manifest" ]] || return 1
  version="$(parse_package_version < "$manifest")"
  [[ -n "$version" && "$version" != "workspace" ]] || return 1
  echo "$version"
}

get_base_explicit_version() {
  local crate="$1"
  local base_manifest
  local version

  if ! base_manifest="$(git show "${BASE_REF}:crates/${crate}/Cargo.toml" 2>/dev/null)"; then
    return 1
  fi

  version="$(printf '%s\n' "$base_manifest" | parse_package_version)"
  [[ -n "$version" && "$version" != "workspace" ]] || return 1
  echo "$version"
}

tier_for_loc() {
  local loc="$1"

  if (( loc <= 10 )); then
    echo "none"
  elif (( loc <= 100 )); then
    echo "patch"
  elif (( loc < 250 )); then
    echo "minor"
  else
    echo "major"
  fi
}

bump_version() {
  local version="$1"
  local tier="$2"
  local major minor patch

  IFS=. read -r major minor patch <<< "$version"
  if ! [[ "$major" =~ ^[0-9]+$ && "$minor" =~ ^[0-9]+$ && "$patch" =~ ^[0-9]+$ ]]; then
    echo "invalid semver: $version" >&2
    return 1
  fi

  case "$tier" in
    patch)
      echo "${major}.${minor}.$((patch + 1))"
      ;;
    minor)
      echo "${major}.$((minor + 1)).1"
      ;;
    major)
      echo "$((major + 1)).0.1"
      ;;
    *)
      echo "unsupported tier: $tier" >&2
      return 1
      ;;
  esac
}

set_package_version() {
  local crate="$1"
  local new_version="$2"
  local manifest="crates/${crate}/Cargo.toml"
  local tmp_manifest="${manifest}.tmp.$$"

  awk -v new_version="$new_version" '
    BEGIN { in_pkg = 0; updated = 0 }
    /^\[package\][[:space:]]*$/ { in_pkg = 1; print; next }
    /^\[/ && in_pkg == 1 { in_pkg = 0 }
    in_pkg == 1 && updated == 0 && /^[[:space:]]*version[[:space:]]*=/ {
      print "version = \"" new_version "\""
      updated = 1
      next
    }
    { print }
    END {
      if (updated == 0) {
        exit 2
      }
    }
  ' "$manifest" > "$tmp_manifest"

  mv "$tmp_manifest" "$manifest"
}

echo "Crate version roll mode: $MODE"
echo "Diff base ref: $BASE_REF"

crate_loc_rows="$(
  git diff --numstat "${BASE_REF}...HEAD" | awk -F '\t' '
    $1 ~ /^[0-9]+$/ && $2 ~ /^[0-9]+$/ {
      path = $3
      if (path ~ /^crates\/[^\/]+\/[^\/]+\.rs$/) {
        split(path, parts, "/")
        crate = parts[2]
        loc[crate] += ($1 + $2)
      }
    }
    END {
      for (crate in loc) {
        printf "%s\t%d\n", crate, loc[crate]
      }
    }
  '
)"

if [[ -z "$crate_loc_rows" ]]; then
  echo "No top-level crate Rust file churn detected. Nothing to enforce."
  exit 0
fi

failures=0
checked=0
autobumped=0

while IFS=$'\t' read -r crate loc; do
  [[ -n "$crate" ]] || continue

  current_version="$(get_current_explicit_version "$crate" || true)"
  if [[ -z "$current_version" ]]; then
    echo "Skipping crates/${crate}/Cargo.toml (no explicit local [package] version)."
    continue
  fi

  tier="$(tier_for_loc "$loc")"
  if [[ "$tier" == "none" ]]; then
    echo "crate=${crate} loc=${loc} tier=none version=${current_version} (no bump required)"
    continue
  fi

  base_version="$(get_base_explicit_version "$crate" || true)"
  if [[ -z "$base_version" ]]; then
    echo "::warning::Unable to resolve explicit base version for crates/${crate}/Cargo.toml at ${BASE_REF}; skipping."
    continue
  fi

  required_version="$(bump_version "$base_version" "$tier")"
  checked=$((checked + 1))

  if [[ "$current_version" == "$required_version" ]]; then
    echo "crate=${crate} loc=${loc} tier=${tier} version=${current_version} (ok)"
    continue
  fi

  if [[ "$MODE" == "autobump" ]]; then
    set_package_version "$crate" "$required_version"
    autobumped=$((autobumped + 1))
    echo "crate=${crate} loc=${loc} tier=${tier} bumped ${current_version} -> ${required_version}"
  else
    failures=$((failures + 1))
    echo "::error::crates/${crate}/Cargo.toml requires a ${tier} bump for ${loc} LOC churn."
    echo "Expected [package] version \"${required_version}\" (from base \"${base_version}\"), found \"${current_version}\"."
    echo "Manual fix: set [package] version in crates/${crate}/Cargo.toml to \"${required_version}\"."
  fi
done < <(printf '%s\n' "$crate_loc_rows" | sort)

if [[ "$MODE" == "autobump" ]]; then
  echo "Checked crates requiring bump: ${checked}"
  echo "Auto-bumped crates: ${autobumped}"
  exit 0
fi

if (( failures > 0 )); then
  echo ""
  echo "Crate version gate failed."
  echo "Apply the manual fixes above and commit, or run:"
  echo "  bash scripts/ci/check_crate_patch_roll.sh --mode autobump --base-ref ${BASE_REF}"
  exit 1
fi

echo "Crate version gate passed."
