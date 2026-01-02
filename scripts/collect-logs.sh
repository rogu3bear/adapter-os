#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'USAGE'
Collect AdapterOS debugging logs/config into a local bundle folder.

Creates: var/log-bundles/<timestamp>/

Usage:
  bash scripts/collect-logs.sh [--lines N] [--db PATH ...]

Options:
  --lines N   Number of log lines to capture per file (default: 2000)
  --db PATH   Additional SQLite DB file to inspect (repeatable)
  -h, --help  Show this help

Environment:
  LOG_LINES        Same as --lines
  AOS_DATABASE_URL If starts with "sqlite:", used to locate DB (not copied)
  DATABASE_URL     If starts with "sqlite:", used to locate DB (not copied)
USAGE
}

info() { printf '%s\n' "$*" >&2; }
warn() { printf 'WARN: %s\n' "$*" >&2; }
die() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

is_positive_int() {
  [[ "${1:-}" =~ ^[0-9]+$ ]] && [[ "$1" -gt 0 ]]
}

redact_stream() {
  local tmp=""
  local tmp_root="${TMPDIR:-}"
  if [[ -z "$tmp_root" || "$tmp_root" == /tmp* || "$tmp_root" == /private/tmp* ]]; then
    tmp_root="${root_dir:-.}/var/tmp"
  fi
  mkdir -p "$tmp_root"
  tmp="$(mktemp "${tmp_root}/aos-redact.XXXXXX")"

  cat >"$tmp" <<'SED'
s/([Aa]uthorization:[[:space:]]*[Bb]earer[[:space:]]+)[^[:space:]]+/\1REDACTED/g
s/([Aa]uthorization:[[:space:]]*[Bb]asic[[:space:]]+)[^[:space:]]+/\1REDACTED/g
s/([Cc]ookie:[[:space:]]*).*/\1REDACTED/g
s/([Xx]-[Aa]pi-[Kk]ey:[[:space:]]*)[^[:space:]]+/\1REDACTED/g
s/([Pp]assword|[Pp]asswd|[Pp]assphrase|[Ss]ecret|[Tt]oken|[Aa]pi[_-]?[Kk]ey|[Aa]ccess[_-]?[Kk]ey|[Cc]lient[_-]?[Ss]ecret)([[:space:]]*[:=][[:space:]]*)("[^"]*"|'[^']*'|[^,;[:space:]]+)/\1\2"REDACTED"/g
s/("[^"]*([Pp]assword|[Pp]asswd|[Pp]assphrase|[Ss]ecret|[Tt]oken|[Aa]pi[_-]?[Kk]ey|[Aa]ccess[_-]?[Kk]ey|[Cc]lient[_-]?[Ss]ecret)[^"]*"[[:space:]]*:[[:space:]]*)("[^"]*"|'[^']*'|[^,}[:space:]]+)/\1"REDACTED"/g
s/^([A-Z0-9_]*(PASSWORD|PASSWD|PASSPHRASE|SECRET|TOKEN|API_KEY|ACCESS_KEY|CLIENT_SECRET|DATABASE_URL|AOS_DATABASE_URL|JWT_SECRET|SIGNING_KEY)[A-Z0-9_]*)[[:space:]]*=[[:space:]]*.*/\1=REDACTED/g
s#(postgres(ql)?://[^:/@]+:)[^@/]+@#\1REDACTED@#g
s#(mysql://[^:/@]+:)[^@/]+@#\1REDACTED@#g
s#(https?://[^:/@]+:)[^@/]+@#\1REDACTED@#g
s/[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}/REDACTED_JWT/g
s/sk-[A-Za-z0-9]{20,}/sk-REDACTED/g
s/([Ss]et-[Cc]ookie:[[:space:]]*[^=]+)=([^;[:space:]]+)/\1=REDACTED/g
/-----BEGIN [A-Z ]*PRIVATE KEY-----/,/-----END [A-Z ]*PRIVATE KEY-----/c\
REDACTED_PRIVATE_KEY_BLOCK
SED

  local rc=0
  sed -E -f "$tmp" || rc=$?
  rm -f "$tmp" || true
  return $rc
}

capture_text_file() {
  local src="$1"
  local dest="$2"

  if [[ ! -r "$src" ]]; then
    warn "Missing/unreadable: $src"
    return 0
  fi

  mkdir -p "$(dirname "$dest")"
  redact_stream <"$src" >"$dest" || true
}

capture_tail_file() {
  local src="$1"
  local dest="$2"
  local lines="$3"

  if [[ ! -r "$src" ]]; then
    warn "Missing/unreadable: $src"
    return 0
  fi

  mkdir -p "$(dirname "$dest")"
  tail -n "$lines" "$src" 2>/dev/null | redact_stream >"$dest" || true
}

capture_command() {
  local dest="$1"
  shift

  mkdir -p "$(dirname "$dest")"
  {
    echo "## Command: $*"
    echo "## Captured at (UTC): $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    echo
    local status=0
    set +e
    "$@"
    status=$?
    set -e
    echo
    echo "## Exit code: $status"
  } 2>&1 | redact_stream >"$dest" || true
}

contains_item() {
  local needle="$1"
  shift
  local item
  for item in "$@"; do
    if [[ "$item" == "$needle" ]]; then
      return 0
    fi
  done
  return 1
}

sqlite_url_to_path() {
  local url="$1"

  if [[ "$url" != sqlite:* ]]; then
    return 1
  fi

  local path="${url#sqlite:}"
  path="${path#//}"
  path="${path%%\?*}"
  path="${path%%\#*}"

  if [[ -z "$path" ]]; then
    return 1
  fi

  printf '%s\n' "$path"
}

main() {
  umask 077

  local root_dir
  root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

  local log_lines="${LOG_LINES:-2000}"
  local extra_dbs
  extra_dbs=()

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --lines)
        [[ $# -ge 2 ]] || die "--lines requires a value"
        log_lines="$2"
        shift 2
        ;;
      --db)
        [[ $# -ge 2 ]] || die "--db requires a path"
        extra_dbs+=("$2")
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        die "Unknown argument: $1 (try --help)"
        ;;
    esac
  done

  is_positive_int "$log_lines" || die "--lines must be a positive integer"

  local ts
  ts="$(date +"%Y%m%d-%H%M%S")"

  local bundle_root="${root_dir}/var/log-bundles"
  local bundle_dir="${bundle_root}/${ts}"

  mkdir -p \
    "$bundle_dir/logs/api" \
    "$bundle_dir/logs/worker" \
    "$bundle_dir/logs/ui" \
    "$bundle_dir/config" \
    "$bundle_dir/db" \
    "$bundle_dir/meta"

  info "Writing bundle to: $bundle_dir"

  capture_command "$bundle_dir/meta/system.txt" uname -a
  capture_command "$bundle_dir/meta/git_rev.txt" git -C "$root_dir" rev-parse HEAD
  capture_command "$bundle_dir/meta/git_status.txt" git -C "$root_dir" status --porcelain=v1

  # ---- Logs (recent tail) ----
  capture_tail_file "$root_dir/var/logs/backend.log" "$bundle_dir/logs/api/backend.log.tail.txt" "$log_lines"
  capture_tail_file "$root_dir/server-dev.log" "$bundle_dir/logs/api/server-dev.log.tail.txt" "$log_lines"
  capture_tail_file "$root_dir/cp.log" "$bundle_dir/logs/api/cp.log.tail.txt" "$log_lines"

  # Capture the most recent aos-cp.<date> logs, if present.
  local cp_rotated
  cp_rotated=()
  if compgen -G "$root_dir/var/logs/aos-cp.*" >/dev/null; then
    local old_ifs="$IFS"
    IFS=$'\n'
    cp_rotated=($(ls -t "$root_dir"/var/logs/aos-cp.* 2>/dev/null | head -n 3))
    IFS="$old_ifs"
  fi
  local cp_log
  if [[ ${#cp_rotated[@]} -gt 0 ]]; then
    for cp_log in "${cp_rotated[@]}"; do
      capture_tail_file "$cp_log" "$bundle_dir/logs/api/$(basename "$cp_log").tail.txt" "$log_lines"
    done
  fi

  capture_tail_file "$root_dir/var/logs/worker.log" "$bundle_dir/logs/worker/worker.log.tail.txt" "$log_lines"
  capture_tail_file "$root_dir/var/logs/ui.log" "$bundle_dir/logs/ui/ui.log.tail.txt" "$log_lines"

  # ---- Config (redacted copies) ----
  capture_text_file "$root_dir/configs/cp.toml" "$bundle_dir/config/cp.toml"
  capture_text_file "$root_dir/.env.example" "$bundle_dir/config/.env.example"
  capture_text_file "$root_dir/.envrc.example" "$bundle_dir/config/.envrc.example"

  # ---- Migrations + SQLite schema/status (no DB data copied) ----
  capture_command "$bundle_dir/db/repo_migrations.txt" bash -c "cd \"$root_dir\" && ls -1 migrations/[0-9][0-9][0-9][0-9]_*.sql 2>/dev/null | sort | tail -n 50"

  local repo_latest_migration=""
  if compgen -G "$root_dir/migrations/[0-9][0-9][0-9][0-9]_*.sql" >/dev/null; then
    repo_latest_migration="$(ls -1 "$root_dir"/migrations/[0-9][0-9][0-9][0-9]_*.sql 2>/dev/null | sort | tail -n 1 || true)"
    if [[ -n "$repo_latest_migration" ]]; then
      repo_latest_migration="$(basename "$repo_latest_migration")"
    fi
  fi

  if [[ -r "$root_dir/var/aos-migrate.checkpoint.json" ]]; then
    capture_text_file "$root_dir/var/aos-migrate.checkpoint.json" "$bundle_dir/db/aos-migrate.checkpoint.json"
  fi

  local sqlite_dbs
  sqlite_dbs=()
  local from_url=""

  from_url="$(sqlite_url_to_path "${AOS_DATABASE_URL:-}" 2>/dev/null || true)"
  [[ -n "$from_url" ]] && sqlite_dbs+=("$from_url")
  from_url="$(sqlite_url_to_path "${DATABASE_URL:-}" 2>/dev/null || true)"
  [[ -n "$from_url" ]] && sqlite_dbs+=("$from_url")

  # Default dev DB locations under var/
  local f
  shopt -s nullglob
  for f in "$root_dir"/var/*.sqlite3; do
    sqlite_dbs+=("$f")
  done
  shopt -u nullglob

  # User-specified DBs
  if [[ ${#extra_dbs[@]} -gt 0 ]]; then
    for f in "${extra_dbs[@]}"; do
      sqlite_dbs+=("$f")
    done
  fi

  # Normalize + de-dupe.
  local normalized_dbs
  normalized_dbs=()
  local db
  if [[ ${#sqlite_dbs[@]} -gt 0 ]]; then
    for db in "${sqlite_dbs[@]}"; do
      [[ -n "$db" ]] || continue

      # Convert sqlite: relative paths (e.g., "var/aos-cp.sqlite3") to absolute.
      if [[ "$db" != /* ]]; then
        db="$root_dir/$db"
      fi

      if [[ ${#normalized_dbs[@]} -eq 0 ]]; then
        normalized_dbs+=("$db")
        continue
      fi

      if ! contains_item "$db" "${normalized_dbs[@]}"; then
        normalized_dbs+=("$db")
      fi
    done
  fi

  if ! command -v sqlite3 >/dev/null 2>&1; then
    warn "sqlite3 not found; skipping DB schema/migration queries"
    printf '%s\n' "sqlite3 not found on PATH; DB schema/migration queries were skipped." \
      >"$bundle_dir/db/sqlite3_missing.txt"
  else
    if [[ ${#normalized_dbs[@]} -gt 0 ]]; then
      for db in "${normalized_dbs[@]}"; do
        [[ -f "$db" ]] || continue

        local db_name
        db_name="$(basename "$db")"
        local out_dir="$bundle_dir/db/$db_name"
        mkdir -p "$out_dir"

        printf '%s\n' "$db" >"$out_dir/path.txt"
        capture_command "$out_dir/file_info.txt" ls -la "$db"
        capture_command "$out_dir/sqlite_version.txt" sqlite3 -version
        capture_command "$out_dir/tables.txt" sqlite3 "$db" "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name;"
        capture_command "$out_dir/schema.sql" sqlite3 "$db" ".schema"
        capture_command "$out_dir/migrations.txt" sqlite3 "$db" "SELECT version, description, installed_on, success, execution_time FROM _sqlx_migrations ORDER BY version DESC LIMIT 20;"
      done
    fi
  fi

  # One-file summary for quick triage.
  {
    echo "Captured at (UTC): $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    echo "Repo latest migration: ${repo_latest_migration:-unknown}"
    echo "Log tail lines per file: $log_lines"
    echo

    if ! command -v sqlite3 >/dev/null 2>&1; then
      echo "sqlite3: not found"
    else
      local any_db=0
      if [[ ${#normalized_dbs[@]} -gt 0 ]]; then
        for db in "${normalized_dbs[@]}"; do
          [[ -f "$db" ]] || continue
          any_db=1
          echo "DB: $db"

          local applied_count=""
          local latest_row=""
          local rc_count=0
          local rc_latest=0

          set +e
          applied_count="$(sqlite3 "$db" "SELECT COUNT(*) FROM _sqlx_migrations;" 2>/dev/null)"
          rc_count=$?
          latest_row="$(sqlite3 "$db" "SELECT version, description, installed_on, success FROM _sqlx_migrations ORDER BY version DESC LIMIT 1;" 2>/dev/null)"
          rc_latest=$?
          set -e

          if [[ $rc_count -eq 0 && -n "$applied_count" ]]; then
            echo "  _sqlx_migrations rows: $applied_count"
          else
            echo "  _sqlx_migrations rows: unavailable"
          fi

          if [[ $rc_latest -eq 0 && -n "$latest_row" ]]; then
            echo "  latest: $latest_row"
          else
            echo "  latest: unavailable"
          fi

          echo
        done
      fi

      if [[ $any_db -eq 0 ]]; then
        echo "SQLite DBs: none found under var/ (or via env/--db)"
      fi
    fi
  } | redact_stream >"$bundle_dir/db/summary.txt" || true

  printf '%s\n' \
    "Bundle created at: $bundle_dir" \
    "Note: files are best-effort redacted; please review before sharing." \
    >"$bundle_dir/README.txt"

  info "Done: $bundle_dir"
}

main "$@"
