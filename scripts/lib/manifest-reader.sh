#!/bin/bash
# adapterOS Manifest Reader
# Pure-awk TOML reader for manifest.toml / cp.toml.
# Handles single-line values, quoted strings, and inline comments.
# No external dependencies beyond awk.
#
# Usage: source scripts/lib/manifest-reader.sh

# Default manifest path (relative to project root)
AOS_MANIFEST_FILE="${AOS_MANIFEST_FILE:-configs/manifest.toml}"

# Check if a manifest file exists and is readable.
# Usage: manifest_exists [path]
manifest_exists() {
    local path="${1:-$AOS_MANIFEST_FILE}"
    [ -f "$path" ] && [ -r "$path" ]
}

# Read a single value from a TOML file.
# Usage: toml_read <file> <section> <key>
# Example: toml_read configs/cp.toml server port  →  8080
#
# Returns the raw value (unquoted for strings, as-is for numbers/booleans).
# Returns empty string and exit 1 if not found.
toml_read() {
    local file="$1"
    local section="$2"
    local key="$3"

    [ -f "$file" ] || return 1

    awk -v section="$section" -v key="$key" '
    BEGIN { in_section = 0; found = 0 }

    # Match [section] headers
    /^[[:space:]]*\[/ {
        # Strip whitespace and brackets
        gsub(/^[[:space:]]*\[/, "")
        gsub(/\][[:space:]]*$/, "")
        gsub(/[[:space:]]/, "")
        if ($0 == section) {
            in_section = 1
        } else {
            in_section = 0
        }
        next
    }

    # Match key = value in current section
    in_section && /^[[:space:]]*[a-zA-Z_][a-zA-Z0-9_.]*[[:space:]]*=/ {
        # Extract key name
        line = $0
        # Strip leading whitespace
        gsub(/^[[:space:]]+/, "", line)
        # Split on first =
        eq_pos = index(line, "=")
        if (eq_pos == 0) next
        k = substr(line, 1, eq_pos - 1)
        v = substr(line, eq_pos + 1)
        # Trim whitespace from key and value
        gsub(/[[:space:]]+$/, "", k)
        gsub(/^[[:space:]]+/, "", v)
        gsub(/[[:space:]]+$/, "", v)

        if (k == key) {
            # Strip inline comments (not inside quotes)
            if (substr(v, 1, 1) == "\"") {
                # Quoted string: find closing quote, ignore everything after
                inner = substr(v, 2)
                close_pos = index(inner, "\"")
                if (close_pos > 0) {
                    v = substr(inner, 1, close_pos - 1)
                } else {
                    # No closing quote — take as-is minus opening quote
                    v = inner
                }
            } else {
                # Unquoted: strip inline comment
                comment_pos = index(v, "#")
                if (comment_pos > 0) {
                    v = substr(v, 1, comment_pos - 1)
                    gsub(/[[:space:]]+$/, "", v)
                }
            }
            print v
            found = 1
            exit
        }
    }

    END { if (!found) exit 1 }
    ' "$file"
}

# Export all keys in a TOML section as environment variables with a prefix.
# Usage: toml_section_to_env <file> <section> <prefix>
# Example: toml_section_to_env configs/manifest.toml boot AOS_BOOT
#   → exports AOS_BOOT_HEALTH_TIMEOUT_SECS=15, etc.
#
# Keys are uppercased and dots replaced with underscores.
# Only sets vars that are not already set (preserves precedence).
toml_section_to_env() {
    local file="$1"
    local section="$2"
    local prefix="$3"

    [ -f "$file" ] || return 1

    local lines
    lines="$(awk -v section="$section" '
    BEGIN { in_section = 0 }

    /^[[:space:]]*\[/ {
        gsub(/^[[:space:]]*\[/, "")
        gsub(/\][[:space:]]*$/, "")
        gsub(/[[:space:]]/, "")
        if ($0 == section) {
            in_section = 1
        } else {
            in_section = 0
        }
        next
    }

    in_section && /^[[:space:]]*[a-zA-Z_][a-zA-Z0-9_.]*[[:space:]]*=/ {
        line = $0
        gsub(/^[[:space:]]+/, "", line)
        eq_pos = index(line, "=")
        if (eq_pos == 0) next
        k = substr(line, 1, eq_pos - 1)
        v = substr(line, eq_pos + 1)
        gsub(/[[:space:]]+$/, "", k)
        gsub(/^[[:space:]]+/, "", v)
        gsub(/[[:space:]]+$/, "", v)

        # Handle value extraction
        if (substr(v, 1, 1) == "\"") {
            inner = substr(v, 2)
            close_pos = index(inner, "\"")
            if (close_pos > 0) {
                v = substr(inner, 1, close_pos - 1)
            } else {
                v = inner
            }
        } else {
            comment_pos = index(v, "#")
            if (comment_pos > 0) {
                v = substr(v, 1, comment_pos - 1)
                gsub(/[[:space:]]+$/, "", v)
            }
        }

        # Uppercase key and replace dots with underscores
        upper_k = k
        gsub(/\./, "_", upper_k)
        cmd = "printf \"%s\" \"" upper_k "\" | tr \"[:lower:]\" \"[:upper:]\""
        cmd | getline upper_k
        close(cmd)

        print upper_k "=" v
    }
    ' "$file")"

    local IFS=$'\n'
    for line in $lines; do
        local var_name="${prefix}_${line%%=*}"
        local var_value="${line#*=}"
        # Only set if not already defined (preserves env > manifest precedence)
        if [ -z "${!var_name+x}" ]; then
            export "$var_name=$var_value"
        fi
    done
}
