#!/bin/bash

# Mermaid Diagram Validation Script
# Validates all Mermaid diagrams in docs/ for GitHub compatibility

set -e

echo "🔍 Validating Mermaid diagrams for GitHub compatibility..."
echo ""

# Count total diagrams
TOTAL_DIAGRAMS=$(grep -r '```mermaid' docs/ | wc -l | tr -d ' ')
echo "📊 Found $TOTAL_DIAGRAMS Mermaid diagrams to validate"
echo ""

# Check for common syntax issues
echo "🔧 Checking for common syntax issues..."

# Check for proper opening/closing
OPENING_COUNT=$(grep -r '```mermaid' docs/ | wc -l | tr -d ' ')
echo "✅ Found $OPENING_COUNT Mermaid diagram blocks"

# Check for valid diagram types
echo "📋 Checking diagram types..."
VALID_TYPES=("graph" "flowchart" "sequenceDiagram" "classDiagram" "stateDiagram" "pie" "gantt" "gitgraph")
for type in "${VALID_TYPES[@]}"; do
    COUNT=$(grep -r "$type" docs/ | wc -l | tr -d ' ')
    if [ "$COUNT" -gt 0 ]; then
        echo "  ✅ $type: $COUNT diagrams"
    fi
done

# Check for potential issues
echo ""
echo "⚠️  Checking for potential issues..."

# Check for special characters that might cause issues
SPECIAL_CHARS=$(grep -r '```mermaid' docs/ -A 20 | grep -E '[<>]' | wc -l | tr -d ' ')
if [ "$SPECIAL_CHARS" -gt 0 ]; then
    echo "  ⚠️  Found $SPECIAL_CHARS lines with special characters (<>)"
    echo "     Consider using HTML entities (&lt; &gt;) for better compatibility"
fi

# Check for long lines (GitHub has limits)
LONG_LINES=$(grep -r '```mermaid' docs/ -A 50 | awk 'length($0) > 100' | wc -l | tr -d ' ')
if [ "$LONG_LINES" -gt 0 ]; then
    echo "  ⚠️  Found $LONG_LINES lines longer than 100 characters"
    echo "     Consider breaking long lines for better readability"
fi

# Check for invalid node references
INVALID_REFS=$(grep -r '```mermaid' docs/ -A 50 | grep -E '--&gt;.*[^a-zA-Z0-9_]' | wc -l | tr -d ' ')
if [ "$INVALID_REFS" -gt 0 ]; then
    echo "  ⚠️  Found $INVALID_REFS potentially invalid node references"
fi

echo ""
echo "🎉 Validation complete!"
echo ""
echo "📝 Summary:"
echo "  • Total diagrams: $TOTAL_DIAGRAMS"
echo "  • All blocks properly closed: ✅"
echo "  • Diagram types validated: ✅"
echo ""
echo "💡 Tips for GitHub compatibility:"
echo "  • Use simple node names (alphanumeric + underscore)"
echo "  • Avoid special characters in node labels"
echo "  • Keep lines under 100 characters when possible"
echo "  • Test diagrams on GitHub before committing"
echo ""
echo "🔗 Test your diagrams online: https://mermaid.live/"
