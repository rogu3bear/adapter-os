#!/bin/bash
# Documentation Validation Script
# Validates documentation files before commit
#
# Usage: ./.github/scripts/validate-docs.sh
# Install as pre-commit: ln -s ../../.github/scripts/validate-docs.sh .git/hooks/pre-commit
#
# Note: This script warns but does not block commits (exit 0 always)

set -e

# Colors for output
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

# Counters
WARNINGS=0
ERRORS=0

# Get script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

cd "$PROJECT_ROOT"

echo "=========================================="
echo "Documentation Validation"
echo "=========================================="
echo ""

# -----------------------------------------------------------------------------
# 1. Naming Convention Check
# Pattern: PURPOSE_COMPONENT.md (with underscore separator)
# Exceptions: README.md, CLAUDE.md, files in archive/
# -----------------------------------------------------------------------------
echo "1. Checking file naming conventions..."

naming_issues=""
while IFS= read -r -d '' file; do
    # Get relative path
    relpath="${file#$PROJECT_ROOT/}"
    filename=$(basename "$file")

    # Skip README.md, CLAUDE.md, CHANGELOG.md, LICENSE.md, CONTRIBUTING.md
    if [[ "$filename" =~ ^(README|CLAUDE|CHANGELOG|LICENSE|CONTRIBUTING)\.md$ ]]; then
        continue
    fi

    # Skip files in archive directories
    if [[ "$relpath" == *"/archive/"* ]]; then
        continue
    fi

    # Skip node_modules
    if [[ "$relpath" == *"node_modules"* ]]; then
        continue
    fi

    # Skip .cursor directory
    if [[ "$relpath" == *".cursor"* ]]; then
        continue
    fi

    # Check for underscore or hyphen separator (PURPOSE_COMPONENT.md or PURPOSE-COMPONENT.md)
    # Valid: SOME_NAME.md, SOME-NAME.md, SOME_NAME_HERE.md
    # Invalid: somename.md (no separator, lowercase)
    if [[ ! "$filename" =~ ^[A-Z0-9]+[-_][A-Z0-9_-]+\.md$ ]] && [[ ! "$filename" =~ ^[A-Z0-9]+\.md$ ]]; then
        # Also allow files like Z_INDEX_HIERARCHY.md (starting with single letter)
        if [[ ! "$filename" =~ ^[A-Z][-_][A-Z0-9_-]+\.md$ ]]; then
            naming_issues+="  - $relpath\n"
        fi
    fi
done < <(find "$PROJECT_ROOT/docs" "$PROJECT_ROOT" -maxdepth 1 -name "*.md" -type f -print0 2>/dev/null)

if [[ -n "$naming_issues" ]]; then
    echo -e "${YELLOW}  Warning: Files not matching naming convention (PURPOSE_COMPONENT.md):${NC}"
    echo -e "$naming_issues"
    ((WARNINGS++))
else
    echo -e "${GREEN}  All files follow naming conventions${NC}"
fi
echo ""

# -----------------------------------------------------------------------------
# 2. Metadata Check
# Verify "Last Updated: YYYY-MM-DD" in docs/ files (non-archive)
# -----------------------------------------------------------------------------
echo "2. Checking metadata (Last Updated field)..."

metadata_missing=""
while IFS= read -r -d '' file; do
    relpath="${file#$PROJECT_ROOT/}"

    # Skip archive files
    if [[ "$relpath" == *"/archive/"* ]]; then
        continue
    fi

    # Skip node_modules
    if [[ "$relpath" == *"node_modules"* ]]; then
        continue
    fi

    # Check for "Last Updated" pattern
    if ! grep -q "Last Updated:" "$file" 2>/dev/null; then
        # Also check for "Last updated:" (case insensitive alternative)
        if ! grep -qi "last updated" "$file" 2>/dev/null; then
            # Check for date pattern in header (some files use different format)
            if ! grep -qE "^\*\*.*[0-9]{4}-[0-9]{2}-[0-9]{2}" "$file" 2>/dev/null; then
                metadata_missing+="  - $relpath\n"
            fi
        fi
    fi
done < <(find "$PROJECT_ROOT/docs" -name "*.md" -type f -print0 2>/dev/null | head -50)

if [[ -n "$metadata_missing" ]]; then
    echo -e "${YELLOW}  Warning: Files missing 'Last Updated' metadata (first 50 checked):${NC}"
    echo -e "$metadata_missing" | head -20
    count=$(echo -e "$metadata_missing" | grep -c "^  -" || true)
    if [[ $count -gt 20 ]]; then
        echo "  ... and $((count - 20)) more"
    fi
    ((WARNINGS++))
else
    echo -e "${GREEN}  All checked files have metadata${NC}"
fi
echo ""

# -----------------------------------------------------------------------------
# 3. Link Validation
# Check [Text](path) links exist (local paths only, skip https://)
# -----------------------------------------------------------------------------
echo "3. Validating internal links..."

broken_links=""
checked_files=0
max_files=100

while IFS= read -r -d '' file; do
    ((checked_files++))
    if [[ $checked_files -gt $max_files ]]; then
        break
    fi

    relpath="${file#$PROJECT_ROOT/}"
    filedir=$(dirname "$file")

    # Skip node_modules
    if [[ "$relpath" == *"node_modules"* ]]; then
        continue
    fi

    # Extract markdown links [text](path)
    while IFS= read -r link; do
        # Skip empty lines
        [[ -z "$link" ]] && continue

        # Skip external URLs
        if [[ "$link" =~ ^https?:// ]] || [[ "$link" =~ ^mailto: ]]; then
            continue
        fi

        # Skip anchor-only links
        if [[ "$link" =~ ^# ]]; then
            continue
        fi

        # Remove anchor from path
        link_path="${link%%#*}"

        # Skip empty paths (pure anchors were already filtered)
        [[ -z "$link_path" ]] && continue

        # Resolve relative path
        if [[ "$link_path" =~ ^/ ]]; then
            # Absolute path from project root
            target_path="$PROJECT_ROOT$link_path"
        else
            # Relative path from file location
            target_path="$filedir/$link_path"
        fi

        # Normalize path (resolve ..)
        target_path=$(cd "$filedir" 2>/dev/null && realpath -m "$link_path" 2>/dev/null || echo "$target_path")

        # Check if target exists
        if [[ ! -e "$target_path" ]]; then
            broken_links+="  - $relpath -> $link\n"
        fi
    done < <(grep -oE '\[[^]]*\]\([^)]+\)' "$file" 2>/dev/null | grep -oE '\([^)]+\)' | tr -d '()' || true)

done < <(find "$PROJECT_ROOT/docs" -name "*.md" -type f -print0 2>/dev/null)

if [[ -n "$broken_links" ]]; then
    echo -e "${YELLOW}  Warning: Broken internal links found:${NC}"
    echo -e "$broken_links" | head -30
    count=$(echo -e "$broken_links" | grep -c "^  -" || true)
    if [[ $count -gt 30 ]]; then
        echo "  ... and $((count - 30)) more"
    fi
    ((WARNINGS++))
else
    echo -e "${GREEN}  All internal links valid (checked $checked_files files)${NC}"
fi
echo ""

# -----------------------------------------------------------------------------
# 4. Deprecation Check
# If doc contains "DEPRECATED", verify it has replacement link
# If doc in /archive/, check for "ARCHIVED" notice
# -----------------------------------------------------------------------------
echo "4. Checking deprecation notices..."

deprecation_issues=""

# Check DEPRECATED files have replacement
while IFS= read -r -d '' file; do
    relpath="${file#$PROJECT_ROOT/}"

    # Skip node_modules
    if [[ "$relpath" == *"node_modules"* ]]; then
        continue
    fi

    if grep -qi "DEPRECATED" "$file" 2>/dev/null; then
        # Check for replacement link or "replaced by" or "use instead"
        if ! grep -qiE "(replaced by|use instead|see also|replacement|migrate to|\]\([^)]+\))" "$file" 2>/dev/null; then
            deprecation_issues+="  - $relpath (DEPRECATED without replacement link)\n"
        fi
    fi
done < <(find "$PROJECT_ROOT/docs" "$PROJECT_ROOT" -maxdepth 1 -name "*.md" -type f -print0 2>/dev/null)

# Check archive files have ARCHIVED notice
while IFS= read -r -d '' file; do
    relpath="${file#$PROJECT_ROOT/}"
    filename=$(basename "$file")

    # Skip README.md in archive dirs
    if [[ "$filename" == "README.md" ]]; then
        continue
    fi

    if ! grep -qiE "(ARCHIVED|ARCHIVE|Historical)" "$file" 2>/dev/null; then
        deprecation_issues+="  - $relpath (in archive/ without ARCHIVED notice)\n"
    fi
done < <(find "$PROJECT_ROOT/docs/archive" -name "*.md" -type f -print0 2>/dev/null | head -20)

if [[ -n "$deprecation_issues" ]]; then
    echo -e "${YELLOW}  Warning: Deprecation/archive issues:${NC}"
    echo -e "$deprecation_issues" | head -20
    ((WARNINGS++))
else
    echo -e "${GREEN}  All deprecation notices properly documented${NC}"
fi
echo ""

# -----------------------------------------------------------------------------
# 5. Index Update Check
# If .md file added to /docs/, check DOCUMENTATION_INDEX.md updated
# -----------------------------------------------------------------------------
echo "5. Verifying index updates..."

index_issues=""

# Get staged .md files in docs/ (if in git context)
if git rev-parse --git-dir > /dev/null 2>&1; then
    staged_docs=$(git diff --cached --name-only --diff-filter=A 2>/dev/null | grep "^docs/.*\.md$" | grep -v "/archive/" || true)

    if [[ -n "$staged_docs" ]]; then
        doc_index="$PROJECT_ROOT/docs/DOCUMENTATION_INDEX.md"
        if [[ -f "$doc_index" ]]; then
            for doc in $staged_docs; do
                docname=$(basename "$doc")
                if ! grep -q "$docname" "$doc_index" 2>/dev/null; then
                    index_issues+="  - $doc not in DOCUMENTATION_INDEX.md\n"
                fi
            done
        fi
    fi

    # Check root .md files
    staged_root=$(git diff --cached --name-only --diff-filter=A 2>/dev/null | grep "^[^/]*\.md$" || true)

    if [[ -n "$staged_root" ]]; then
        # Root docs should be mentioned in main README or CLAUDE.md References section
        readme="$PROJECT_ROOT/README.md"
        claude="$PROJECT_ROOT/CLAUDE.md"
        for doc in $staged_root; do
            docname=$(basename "$doc")
            # Skip standard files
            if [[ "$docname" =~ ^(README|CLAUDE|CHANGELOG|LICENSE|CONTRIBUTING)\.md$ ]]; then
                continue
            fi
            found=false
            if [[ -f "$readme" ]] && grep -q "$docname" "$readme" 2>/dev/null; then
                found=true
            fi
            if [[ -f "$claude" ]] && grep -q "$docname" "$claude" 2>/dev/null; then
                found=true
            fi
            if [[ "$found" == "false" ]]; then
                index_issues+="  - $doc not referenced in README.md or CLAUDE.md\n"
            fi
        done
    fi
fi

if [[ -n "$index_issues" ]]; then
    echo -e "${YELLOW}  Warning: New docs may need index updates:${NC}"
    echo -e "$index_issues"
    ((WARNINGS++))
else
    echo -e "${GREEN}  Index files appear up to date${NC}"
fi
echo ""

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
echo "=========================================="
echo "Validation Summary"
echo "=========================================="

if [[ $WARNINGS -gt 0 ]]; then
    echo -e "${YELLOW}Warnings: $WARNINGS${NC}"
    echo ""
    echo "These are warnings only - commit will proceed."
    echo "Consider fixing these issues for better documentation quality."
else
    echo -e "${GREEN}All checks passed!${NC}"
fi

echo ""
echo "=========================================="

# Always exit 0 - warnings don't block commit
exit 0
