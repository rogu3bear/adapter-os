# Database Schema Documentation Validation

## Overview

Validation framework for ensuring the accuracy, consistency, and quality of database schema documentation and animated workflows.

## Validation Types

### 1. Mermaid Syntax Validation

#### Syntax Checker
All Mermaid diagrams must use valid syntax and render correctly in documentation viewers.

**Validation Steps**:
1. Extract all Mermaid code blocks from markdown files
2. Validate syntax using Mermaid CLI or online validator
3. Ensure consistent styling and formatting
4. Check for proper error handling

**Example Validation Command**:
```bash
# Using mermaid-cli (mmdc)
mmdc -i diagram.md -o diagram.png

# Or use online validator
# https://mermaid.live/
```

#### Common Syntax Issues
- Missing closing braces in subgraphs
- Invalid arrow syntax in flowcharts
- Incorrect participant names in sequence diagrams
- Undefined style references

### 2. Content Validation

#### Database Table References
All table names referenced in documentation must match actual database schema.

**Validation Query**:
```sql
-- Get all tables in database
SELECT name FROM sqlite_master 
WHERE type='table' 
ORDER BY name;
```

**Check List**:
- [ ] All table names in workflows match database schema
- [ ] All column names are accurate and current
- [ ] Foreign key relationships are correctly documented
- [ ] Primary keys and unique constraints are noted

#### Field Documentation
All fields referenced must exist and have correct data types.

**Validation Steps**:
1. Extract field names from workflow documents
2. Compare with actual table schema
3. Verify data types match documentation
4. Check for deprecated fields

### 3. Workflow Validation

#### Process Accuracy
Workflows must reflect actual operational processes.

**Verification Steps**:
1. **Adapter Lifecycle**: Test adapter state transitions
2. **Promotion Pipeline**: Verify deployment steps
3. **Monitoring Flow**: Confirm metrics collection
4. **Security & Compliance**: Test signature verification

**Test Commands**:
```bash
# Test adapter lifecycle
sqlite3 var/aos-cp.sqlite3 "SELECT id, current_state FROM adapters;"

# Test promotion process
sqlite3 var/aos-cp.sqlite3 "SELECT * FROM promotions ORDER BY promoted_at DESC LIMIT 5;"

# Test monitoring metrics
sqlite3 var/aos-cp.sqlite3 "SELECT * FROM system_metrics ORDER BY timestamp DESC LIMIT 1;"
```

### 4. Cross-Reference Validation

#### Internal Links
All cross-references to other documentation must be valid.

**Check Script**:
```bash
# Find all markdown links
grep -r "\[.*\](.*\.md)" docs/database-schema/

# Verify files exist
for link in $(grep -oh "](.*\.md)" docs/database-schema/ -r | sed 's/](\|)//g'); do
  if [ ! -f "docs/database-schema/$link" ] && [ ! -f "docs/$link" ]; then
    echo "Broken link: $link"
  fi
done
```

#### External References
Links to external documentation (crates, API endpoints) must be accurate.

**Verification**:
- [ ] Rust crate references point to existing files
- [ ] API endpoint paths match actual routes
- [ ] Related documentation links are valid

### 5. Animation Quality

#### Visual Clarity
Diagrams must be clear, readable, and not overly complex.

**Quality Criteria**:
- Maximum 10-12 nodes/participants per diagram
- Clear flow direction (top-to-bottom or left-to-right)
- Consistent color scheme and styling
- Appropriate labels and notes
- Mobile-friendly rendering

#### Performance
Diagrams must render quickly without causing browser lag.

**Performance Tests**:
- Load time < 1 second for complex diagrams
- No excessive DOM nodes
- Efficient SVG rendering
- Responsive on mobile devices

### 6. Documentation Completeness

#### Required Sections
Each workflow document must include:
- [ ] Overview section
- [ ] Workflow animation (Mermaid diagram)
- [ ] Database tables involved with key fields
- [ ] Related workflows section
- [ ] Related documentation section
- [ ] Implementation references (where applicable)

#### Metadata
Each document should have:
- [ ] Last updated date
- [ ] Document status (complete/draft)
- [ ] Author/maintainer information (optional)
- [ ] Version number (optional)

## Automated Validation

### Pre-commit Hook
```bash
#!/bin/bash
# .git/hooks/pre-commit

# Validate Mermaid syntax
for file in $(git diff --cached --name-only | grep "database-schema.*\.md$"); do
  if grep -q "```mermaid" "$file"; then
    echo "Validating Mermaid diagrams in $file..."
    # Add validation logic here
  fi
done

# Check for broken links
echo "Checking for broken links..."
./scripts/validate-links.sh docs/database-schema/

# Verify table references
echo "Verifying database table references..."
./scripts/verify-schema-refs.sh

exit 0
```

### CI/CD Integration
```yaml
# .github/workflows/validate-docs.yml
name: Validate Documentation

on: [push, pull_request]

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      
      - name: Install Mermaid CLI
        run: npm install -g @mermaid-js/mermaid-cli
      
      - name: Validate Mermaid Diagrams
        run: |
          find docs/database-schema -name "*.md" -exec sh -c '
            if grep -q "```mermaid" "$1"; then
              echo "Validating $1..."
              # Extract and validate Mermaid blocks
            fi
          ' _ {} \;
      
      - name: Check Links
        run: ./scripts/validate-links.sh docs/database-schema/
      
      - name: Verify Schema References
        run: ./scripts/verify-schema-refs.sh
```

## Manual Validation Checklist

### Before Committing Changes
- [ ] All Mermaid diagrams render correctly
- [ ] Database table names are accurate
- [ ] Field names and types are current
- [ ] Workflows reflect actual processes
- [ ] Cross-references are valid
- [ ] No broken links
- [ ] Documentation is complete
- [ ] Examples are tested and work

### Monthly Review
- [ ] Review for outdated information
- [ ] Update with schema changes
- [ ] Test all example queries
- [ ] Verify automation references
- [ ] Check for new features to document

### Quarterly Audit
- [ ] Full validation run
- [ ] User feedback incorporation
- [ ] Performance review
- [ ] Accessibility check
- [ ] Update version numbers

## Validation Scripts

### Schema Reference Validator
```python
#!/usr/bin/env python3
# scripts/verify-schema-refs.py

import re
import sqlite3
import sys

def get_tables(db_path):
    conn = sqlite3.connect(db_path)
    cursor = conn.execute("SELECT name FROM sqlite_master WHERE type='table'")
    tables = {row[0] for row in cursor.fetchall()}
    conn.close()
    return tables

def find_table_refs(file_path):
    with open(file_path, 'r') as f:
        content = f.read()
    # Extract table names from markdown
    pattern = r'`(\w+)` table|#### `(\w+)`|FROM (\w+)|JOIN (\w+)'
    matches = re.findall(pattern, content)
    return {m for group in matches for m in group if m}

def validate_refs(db_path, docs_path):
    actual_tables = get_tables(db_path)
    referenced_tables = find_table_refs(docs_path)
    
    invalid = referenced_tables - actual_tables
    if invalid:
        print(f"Invalid table references: {invalid}")
        sys.exit(1)
    print("All table references valid!")

if __name__ == '__main__':
    validate_refs('var/aos-cp.sqlite3', 'docs/database-schema/')
```

### Link Checker
```bash
#!/bin/bash
# scripts/validate-links.sh

DOCS_DIR="${1:-docs/database-schema}"
BROKEN_LINKS=0

echo "Checking links in $DOCS_DIR..."

# Find all markdown files
find "$DOCS_DIR" -name "*.md" | while read -r file; do
  # Extract links
  grep -oP '\[.*?\]\(\K[^)]+' "$file" | while read -r link; do
    # Skip external URLs
    if [[ "$link" =~ ^https?:// ]]; then
      continue
    fi
    
    # Resolve relative path
    dir=$(dirname "$file")
    target="$dir/$link"
    
    if [ ! -f "$target" ]; then
      echo "Broken link in $file: $link"
      BROKEN_LINKS=$((BROKEN_LINKS + 1))
    fi
  done
done

if [ $BROKEN_LINKS -gt 0 ]; then
  echo "Found $BROKEN_LINKS broken links"
  exit 1
fi

echo "All links valid!"
exit 0
```

## Error Reporting

### Issue Template
```markdown
## Documentation Validation Error

**File**: `docs/database-schema/workflows/example.md`

**Error Type**: [Syntax|Content|Link|Format]

**Description**: 
Brief description of the validation error

**Expected**:
What the correct state should be

**Actual**:
What was found

**Steps to Reproduce**:
1. Step one
2. Step two

**Priority**: [Critical|High|Medium|Low]
```

## Maintenance Schedule

### Weekly
- Run automated validation
- Check for new broken links
- Review recent changes

### Monthly
- Full manual validation
- Update documentation as needed
- Test all examples

### Quarterly
- Comprehensive audit
- User feedback review
- Performance optimization

---

**Validation Framework**: Ensuring accuracy, consistency, and quality of database schema documentation through automated and manual validation procedures.
