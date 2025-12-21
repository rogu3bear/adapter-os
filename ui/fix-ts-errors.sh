#!/bin/bash

# This script fixes common TypeScript strict mode errors across the codebase

# Get all files with TypeScript errors
files=$(pnpm tsc --noEmit 2>&1 | grep "error TS" | cut -d'(' -f1 | sort | uniq)

echo "Files with errors:"
echo "$files"
echo ""
echo "Total files: $(echo "$files" | wc -l)"
