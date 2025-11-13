#!/bin/bash

# Script to update AdapterOS version to alpha-v0.04-unstable

set -e

NEW_VERSION="alpha-v0.04-unstable"

echo "Updating AdapterOS version to $NEW_VERSION"

# Update VERSION file
echo "$NEW_VERSION" > VERSION

# Update all .md files
find . -name "*.md" -type f -exec sed -i '' "s/v0\.3-alpha/$NEW_VERSION/g" {} \;
find . -name "*.md" -type f -exec sed -i '' "s/alpha-v0\.01-1/$NEW_VERSION/g" {} \;

echo "Version update complete!"
echo "Updated VERSION file to: $NEW_VERSION"
echo "Updated all .md files containing version references"
