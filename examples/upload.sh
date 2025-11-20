#!/bin/bash

##############################################################################
# AdapterOS Upload Script
#
# Comprehensive bash script for uploading .aos adapters to AdapterOS
# with validation, error handling, and retry logic.
#
# Usage:
#   ./upload.sh <file.aos> <adapter-name> [options]
#   ./upload.sh adapter.aos "My Adapter" --tier persistent --rank 16
#
# Environment:
#   API_URL: Server URL (default: http://localhost:8080)
#   JWT_TOKEN: Bearer token (required)
#
# Options:
#   --tier {ephemeral|warm|persistent}  Lifecycle tier (default: ephemeral)
#   --category {general|code|...}       Adapter category (default: general)
#   --rank <1-512>                      LoRA rank (default: 1)
#   --alpha <0.0-100.0>                 LoRA scaling (default: 1.0)
#   --description "text"                Adapter description
#   --retry <N>                         Retry attempts (default: 3)
#   --timeout <seconds>                 Request timeout (default: 60)
#   --verbose                           Verbose output
#
##############################################################################

set -euo pipefail

# Configuration
API_URL="${API_URL:-http://localhost:8080}"
JWT_TOKEN="${JWT_TOKEN:-}"

# Default parameters
FILE=""
NAME=""
TIER="ephemeral"
CATEGORY="general"
SCOPE="general"
RANK=1
ALPHA=1.0
DESCRIPTION=""
RETRY_COUNT=3
TIMEOUT=60
VERBOSE=0

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

##############################################################################
# Utility Functions
##############################################################################

log_info() {
    echo -e "${BLUE}ℹ${NC} $*"
}

log_success() {
    echo -e "${GREEN}✓${NC} $*"
}

log_error() {
    echo -e "${RED}✗${NC} $*" >&2
}

log_warn() {
    echo -e "${YELLOW}⚠${NC} $*" >&2
}

verbose() {
    if [ "$VERBOSE" = 1 ]; then
        echo -e "${BLUE}[DEBUG]${NC} $*" >&2
    fi
}

die() {
    log_error "$*"
    exit 1
}

##############################################################################
# Validation Functions
##############################################################################

validate_file_exists() {
    local file=$1
    [ -f "$file" ] || die "File not found: $file"
    verbose "File exists: $file"
}

validate_file_extension() {
    local file=$1
    [[ "$file" == *.aos ]] || die "File must have .aos extension, got: $file"
    verbose "File extension is .aos"
}

validate_file_size() {
    local file=$1
    local size
    size=$(stat -c%s "$file" 2>/dev/null || stat -f%z "$file")

    local max=$((1024 * 1024 * 1024))
    if [ "$size" -gt "$max" ]; then
        die "File too large: $((size / 1024 / 1024))MB (max: 1024MB)"
    fi

    local size_mb=$((size / 1024 / 1024))
    verbose "File size OK: ${size_mb}MB"
}

validate_aos_structure() {
    local file=$1

    # Check header (8 bytes)
    local file_size
    file_size=$(stat -c%s "$file" 2>/dev/null || stat -f%z "$file")

    if [ "$file_size" -lt 8 ]; then
        die "File too small for .aos header (need 8+ bytes)"
    fi

    verbose ".aos structure looks valid"
}

validate_token() {
    local token=$1
    [ -n "$token" ] || die "JWT_TOKEN not set"
    [[ "$token" =~ ^eyJ ]] || log_warn "Token doesn't look like JWT (starts with eyJ)"
    verbose "JWT token provided"
}

validate_parameters() {
    # Validate rank
    if ! [[ "$RANK" =~ ^[0-9]+$ ]] || [ "$RANK" -lt 1 ] || [ "$RANK" -gt 512 ]; then
        die "Rank must be integer 1-512, got: $RANK"
    fi
    verbose "Rank valid: $RANK"

    # Validate alpha (simplified check)
    if ! [[ "$ALPHA" =~ ^[0-9]*\.?[0-9]+$ ]]; then
        die "Alpha must be numeric, got: $ALPHA"
    fi
    verbose "Alpha valid: $ALPHA"

    # Validate tier
    case "$TIER" in
        ephemeral|warm|persistent) ;;
        *) die "Invalid tier: $TIER (must be ephemeral, warm, or persistent)" ;;
    esac
    verbose "Tier valid: $TIER"

    # Validate category
    case "$CATEGORY" in
        general|code|text|vision|audio) ;;
        *) die "Invalid category: $CATEGORY" ;;
    esac
    verbose "Category valid: $CATEGORY"

    # Validate scope
    case "$SCOPE" in
        general|public|private|tenant) ;;
        *) die "Invalid scope: $SCOPE" ;;
    esac
    verbose "Scope valid: $SCOPE"
}

##############################################################################
# Upload Functions
##############################################################################

perform_upload() {
    local file=$1
    local name=$2
    local attempt=$3

    verbose "Upload attempt $attempt/$RETRY_COUNT"

    local curl_opts=(
        --silent
        --show-error
        --write-out "\n%{http_code}"
        --max-time "$TIMEOUT"
    )

    # Build multipart form
    local form_args=(
        -F "file=@$file"
        -F "name=$name"
        -F "tier=$TIER"
        -F "category=$CATEGORY"
        -F "scope=$SCOPE"
        -F "rank=$RANK"
        -F "alpha=$ALPHA"
    )

    if [ -n "$DESCRIPTION" ]; then
        form_args+=(-F "description=$DESCRIPTION")
    fi

    verbose "Uploading to: $API_URL/v1/adapters/upload-aos"
    verbose "File size: $(du -h "$file" | cut -f1)"

    # Perform upload
    local response
    response=$(curl "${curl_opts[@]}" \
        -X POST "$API_URL/v1/adapters/upload-aos" \
        -H "Authorization: Bearer $JWT_TOKEN" \
        "${form_args[@]}" 2>&1)

    # Extract HTTP code
    local http_code
    http_code=$(echo "$response" | tail -n1)
    local body
    body=$(echo "$response" | head -n-1)

    verbose "HTTP $http_code"

    echo "$body"
}

handle_upload_response() {
    local http_code=$1
    local body=$2

    case "$http_code" in
        200)
            local adapter_id
            adapter_id=$(echo "$body" | grep -o '"adapter_id":"[^"]*"' | cut -d'"' -f4)
            local hash
            hash=$(echo "$body" | grep -o '"hash_b3":"[^"]*"' | cut -d'"' -f4)
            local state
            state=$(echo "$body" | grep -o '"lifecycle_state":"[^"]*"' | cut -d'"' -f4)

            log_success "Upload successful!"
            log_info "Adapter ID: $adapter_id"
            log_info "Hash: $hash"
            log_info "State: $state"

            echo "$body"
            return 0
            ;;

        400)
            local error_code
            error_code=$(echo "$body" | grep -o '"error_code":"[^"]*"' | cut -d'"' -f4)
            local message
            message=$(echo "$body" | grep -o '"message":"[^"]*"' | cut -d'"' -f4)

            log_error "Validation error: $error_code"
            log_error "Message: $message"
            return 1
            ;;

        403)
            log_error "Permission denied - need Admin or Operator role"
            return 1
            ;;

        409)
            log_error "Adapter ID conflict (UUID collision, retry)"
            return 1
            ;;

        413)
            log_error "File too large for endpoint (max 1GB)"
            return 1
            ;;

        507)
            log_error "Server disk space exhausted (retryable)"
            return 2  # Retryable
            ;;

        500)
            log_error "Internal server error (retryable)"
            return 2  # Retryable
            ;;

        *)
            log_error "Upload failed with HTTP $http_code"
            verbose "Response: $body"
            return 1
            ;;
    esac
}

upload_with_retry() {
    local file=$1
    local name=$2

    for attempt in $(seq 1 "$RETRY_COUNT"); do
        log_info "Attempting upload... ($attempt/$RETRY_COUNT)"

        response=$(perform_upload "$file" "$name" "$attempt")
        http_code=$(echo "$response" | tail -n1)
        body=$(echo "$response" | head -n-1)

        if handle_upload_response "$http_code" "$body"; then
            return 0
        fi

        local status=$?
        if [ $status -ne 2 ]; then
            # Non-retryable error
            return 1
        fi

        # Retryable error - wait before retry
        if [ "$attempt" -lt "$RETRY_COUNT" ]; then
            local delay=$((2 ** attempt))
            log_warn "Retryable error, waiting ${delay}s before retry..."
            sleep "$delay"
        fi
    done

    log_error "Upload failed after $RETRY_COUNT attempts"
    return 1
}

##############################################################################
# Parse Arguments
##############################################################################

parse_arguments() {
    [ $# -lt 2 ] && usage

    FILE="$1"
    NAME="$2"
    shift 2

    while [ $# -gt 0 ]; do
        case "$1" in
            --tier)
                TIER="$2"
                shift 2
                ;;
            --category)
                CATEGORY="$2"
                shift 2
                ;;
            --rank)
                RANK="$2"
                shift 2
                ;;
            --alpha)
                ALPHA="$2"
                shift 2
                ;;
            --description)
                DESCRIPTION="$2"
                shift 2
                ;;
            --retry)
                RETRY_COUNT="$2"
                shift 2
                ;;
            --timeout)
                TIMEOUT="$2"
                shift 2
                ;;
            --verbose)
                VERBOSE=1
                shift
                ;;
            --help)
                usage
                ;;
            *)
                die "Unknown option: $1"
                ;;
        esac
    done
}

usage() {
    cat << EOF
Usage: $(basename "$0") <file.aos> <adapter-name> [options]

Examples:
  $(basename "$0") adapter.aos "My Adapter"
  $(basename "$0") adapter.aos "Code Review" --tier persistent --rank 16
  $(basename "$0") adapter.aos "Test" --category code --alpha 8.0 --verbose

Options:
  --tier {ephemeral|warm|persistent}  Lifecycle tier (default: ephemeral)
  --category {general|code|...}       Adapter category (default: general)
  --rank <1-512>                      LoRA rank (default: 1)
  --alpha <0.0-100.0>                 LoRA scaling (default: 1.0)
  --description "text"                Adapter description
  --retry <N>                         Retry attempts (default: 3)
  --timeout <seconds>                 Request timeout (default: 60)
  --verbose                           Verbose output
  --help                              Show this help

Environment:
  API_URL: Server URL (default: http://localhost:8080)
  JWT_TOKEN: Bearer token (required)

EOF
    exit 0
}

##############################################################################
# Main
##############################################################################

main() {
    log_info "AdapterOS Upload Script"

    # Parse arguments
    parse_arguments "$@"

    # Validate environment
    [ -n "$JWT_TOKEN" ] || die "JWT_TOKEN environment variable required"
    verbose "API URL: $API_URL"

    # Validate file
    log_info "Validating file..."
    validate_file_exists "$FILE"
    validate_file_extension "$FILE"
    validate_file_size "$FILE"
    validate_aos_structure "$FILE"

    # Validate token
    log_info "Validating token..."
    validate_token "$JWT_TOKEN"

    # Validate parameters
    log_info "Validating parameters..."
    validate_parameters

    # Perform upload
    log_info "Starting upload..."
    if upload_with_retry "$FILE" "$NAME"; then
        log_success "All steps completed successfully!"
        return 0
    else
        log_error "Upload failed"
        return 1
    fi
}

# Run main function with all arguments
main "$@"
