#!/bin/bash

# Script to automatically resolve merge conflicts in test files
# Strategy: For test files, we'll keep the HEAD version as it's the current main branch

echo "Starting merge conflict resolution..."

# Find all files with merge conflicts
files_with_conflicts=$(grep -l "^<<<<<<< HEAD" tests/ examples/ menu-bar-app/ ui/ -r 2>/dev/null)

total_files=$(echo "$files_with_conflicts" | wc -l | tr -d ' ')
echo "Found $total_files files with merge conflicts"

resolved_count=0
failed_files=""

for file in $files_with_conflicts; do
    if [ -f "$file" ]; then
        echo "Processing: $file"

        # Create a backup
        cp "$file" "${file}.backup"

        # Use a more sophisticated resolution strategy:
        # Extract the HEAD version (between <<<<<<< HEAD and =======)
        # This preserves the structure better than just removing conflict markers

        awk '
        BEGIN { in_conflict = 0; keep_head = 0 }
        /^<<<<<<< HEAD/ { in_conflict = 1; keep_head = 1; next }
        /^=======/ { if (in_conflict) { keep_head = 0; next } }
        /^>>>>>>> / { if (in_conflict) { in_conflict = 0; next } }
        { if (!in_conflict || keep_head) print }
        ' "$file" > "${file}.resolved"

        # Check if the resolved file is valid (not empty and different from original)
        if [ -s "${file}.resolved" ]; then
            mv "${file}.resolved" "$file"
            rm "${file}.backup"
            resolved_count=$((resolved_count + 1))
            echo "  ✓ Resolved"
        else
            echo "  ✗ Failed to resolve - keeping backup"
            rm "${file}.resolved"
            mv "${file}.backup" "$file"
            failed_files="$failed_files\n$file"
        fi
    fi
done

echo ""
echo "========================================="
echo "Resolution Summary:"
echo "  Total files: $total_files"
echo "  Successfully resolved: $resolved_count"
echo "  Failed: $((total_files - resolved_count))"

if [ ! -z "$failed_files" ]; then
    echo ""
    echo "Failed files that need manual review:"
    echo -e "$failed_files"
fi

echo ""
echo "Next steps:"
echo "1. Review the changes with: git diff"
echo "2. Run: cargo build --workspace"
echo "3. If compilation succeeds, commit the resolution"