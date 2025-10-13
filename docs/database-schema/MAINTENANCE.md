# Database Schema Documentation Maintenance

## Overview

Procedures and guidelines for maintaining database schema documentation and animated workflows to ensure they remain accurate, current, and useful.

## Maintenance Responsibilities

### Documentation Maintainers
- Review and approve documentation changes
- Ensure consistency across all documents
- Coordinate with schema changes
- Respond to user feedback

### Schema Developers
- Update documentation when schema changes
- Notify maintainers of changes
- Test examples before committing
- Provide migration guidance

### All Contributors
- Follow established patterns
- Run validation before committing
- Update cross-references
- Document breaking changes

## Regular Maintenance Tasks

### Weekly Tasks

#### 1. Monitor for Issues
- Check for reported documentation errors
- Review user feedback and questions
- Scan for broken links
- Monitor schema change notifications

#### 2. Quick Validation
```bash
# Run automated validation
./scripts/validate-links.sh docs/database-schema/
./scripts/verify-schema-refs.sh

# Check recent changes
git log --since="1 week ago" -- docs/database-schema/
```

#### 3. Update Status
- Mark outdated sections for review
- Track pending updates
- Close resolved issues

### Monthly Tasks

#### 1. Comprehensive Review
- Review all workflow animations
- Verify examples still work
- Check for outdated information
- Update version numbers

#### 2. Schema Alignment Check
```sql
-- Compare documented tables with actual schema
SELECT name FROM sqlite_master 
WHERE type='table' 
  AND name NOT IN (
    -- List of documented tables
    'users', 'tenants', 'jwt_secrets', 'nodes', 'workers',
    'models', 'artifacts', 'bundle_signatures',
    'adapters', 'ephemeral_adapters', 'adapter_provenance',
    'manifests', 'plans', 'cp_pointers', 'promotions',
    'policies', 'code_policies', 'audits',
    'repositories', 'commits', 'patch_proposals',
    'jobs', 'alerts', 'incidents',
    'telemetry_bundles', 'system_metrics', 'system_health_checks',
    'threshold_violations', 'metrics_aggregations', 'system_metrics_config',
    'replication_journal', 'replication_artifacts',
    'enclave_operations', 'key_metadata',
    'adapter_categories', 'adapter_scopes', 'adapter_states'
  )
ORDER BY name;
```

#### 3. User Feedback Review
- Analyze documentation usage patterns
- Incorporate user suggestions
- Update based on common questions
- Improve clarity where needed

### Quarterly Tasks

#### 1. Full Documentation Audit
- Validate all Mermaid diagrams
- Test all SQL examples
- Review all cross-references
- Check for missing documentation

#### 2. Performance Review
- Measure documentation load times
- Optimize large diagrams
- Improve mobile rendering
- Update caching strategies

#### 3. Accessibility Audit
- Check contrast ratios
- Verify screen reader compatibility
- Ensure keyboard navigation
- Add alt text where needed

## Updating Documentation

### When Schema Changes

#### 1. Assess Impact
```bash
# Identify affected documentation
grep -r "table_name" docs/database-schema/
```

#### 2. Update Documentation
- Update schema diagram with new tables/fields
- Modify affected workflow animations
- Update examples and queries
- Fix broken references

#### 3. Add Migration Notes
```markdown
## Schema Change: 2025-10-09

### Added
- New `example_table` with fields: `id`, `name`, `created_at`

### Modified
- `adapters.new_field` added (default: NULL)

### Deprecated
- `old_table.legacy_field` will be removed in next major version

### Migration
```sql
-- Add new field to adapters
ALTER TABLE adapters ADD COLUMN new_field TEXT;
```
```

#### 4. Update Workflow Animations
If schema changes affect workflows:
1. Update relevant Mermaid diagrams
2. Add notes about new processes
3. Update database tables sections
4. Test new workflows

### When Workflows Change

#### 1. Document Changes
- Update workflow animation
- Revise process descriptions
- Update examples
- Add migration guide

#### 2. Validate Changes
- Test new workflow end-to-end
- Verify all queries work
- Check for side effects
- Update related workflows

#### 3. Communicate Changes
- Add changelog entry
- Notify affected users
- Update training materials
- Create migration guide

## Documentation Patterns

### File Structure
```
docs/database-schema/
├── README.md                    # Main index
├── schema-diagram.md             # Static ER diagram
├── VALIDATION.md                # This file
├── MAINTENANCE.md               # Maintenance guide
├── workflows/                   # Animated workflows
│   ├── adapter-lifecycle.md
│   ├── promotion-pipeline.md
│   ├── monitoring-flow.md
│   ├── security-compliance.md
│   ├── replication-distribution.md
│   ├── code-intelligence.md
│   ├── performance-dashboard.md
│   └── incident-response.md
└── examples/                    # Usage examples
    └── basic-workflows.md
```

### Naming Conventions
- Use kebab-case for filenames: `adapter-lifecycle.md`
- Use descriptive names: `promotion-pipeline.md` not `pp.md`
- Group related files in subdirectories
- Use consistent prefixes for related files

### Content Structure
```markdown
# Workflow Title

## Overview
Brief description of what this workflow shows

## Workflow Animation
```mermaid
[Diagram code]
```

## Database Tables Involved
### Primary Tables
- Detailed field descriptions
### Supporting Tables
- Brief mentions

## Related Workflows
- Links to related documentation

## Related Documentation
- Links to broader documentation

## Implementation References
- Rust crates and API endpoints

---
Footer with summary
```

## Version Control

### Commit Messages
```
docs(schema): Update adapter lifecycle workflow

- Add memory pressure handling
- Update state transition diagram
- Fix broken link to promotion pipeline
- Add example for ephemeral adapters
```

### Branching Strategy
- `main` - Stable documentation
- `docs/schema-updates` - Documentation changes
- `feature/new-workflow` - New workflow additions

### Pull Request Template
```markdown
## Documentation Update

**Type**: [New|Update|Fix|Refactor]

**Files Changed**:
- `docs/database-schema/workflows/example.md`

**Description**:
Brief description of changes

**Validation**:
- [ ] Mermaid diagrams render correctly
- [ ] All links are valid
- [ ] Examples tested
- [ ] Cross-references updated

**Related Issues**: #123
```

## Troubleshooting

### Common Issues

#### Broken Mermaid Diagrams
**Symptom**: Diagram doesn't render
**Solution**:
1. Validate syntax at https://mermaid.live/
2. Check for unclosed brackets
3. Verify participant/node names
4. Test with mermaid-cli locally

#### Outdated Examples
**Symptom**: SQL queries fail
**Solution**:
1. Verify schema changes
2. Update table/field names
3. Test against current database
4. Update documentation

#### Broken Links
**Symptom**: 404 errors
**Solution**:
1. Run link checker script
2. Update moved/renamed files
3. Fix relative paths
4. Test in documentation viewer

#### Performance Issues
**Symptom**: Slow diagram rendering
**Solution**:
1. Simplify complex diagrams
2. Split into multiple diagrams
3. Reduce node count
4. Optimize styling

## Documentation Tools

### Required Tools
- **Mermaid CLI**: For validating diagrams
- **SQLite3**: For testing queries
- **Markdown linter**: For format consistency
- **Link checker**: For broken link detection

### Installation
```bash
# Mermaid CLI
npm install -g @mermaid-js/mermaid-cli

# SQLite3 (usually pre-installed on macOS/Linux)
brew install sqlite3  # macOS
apt-get install sqlite3  # Linux

# Markdown linter
npm install -g markdownlint-cli

# Link checker
pip install linkchecker
```

### Usage Examples
```bash
# Validate Mermaid diagram
mmdc -i workflow.md -o workflow.png

# Test SQL query
sqlite3 var/aos-cp.sqlite3 < test-query.sql

# Lint markdown
markdownlint docs/database-schema/**/*.md

# Check links
linkchecker docs/database-schema/
```

## Backup and Recovery

### Documentation Backups
- Version controlled in Git
- Regular backups via CI/CD
- Archive old versions
- Track major releases

### Recovery Procedures
```bash
# Restore from Git
git checkout main -- docs/database-schema/

# Restore specific file
git checkout HEAD~5 -- docs/database-schema/workflows/example.md

# View history
git log --follow docs/database-schema/workflows/example.md
```

## Contributing Guidelines

### Before Making Changes
1. Read existing documentation
2. Follow established patterns
3. Test changes locally
4. Run validation scripts

### Making Changes
1. Create feature branch
2. Make focused changes
3. Update cross-references
4. Test thoroughly

### Submitting Changes
1. Run validation
2. Create pull request
3. Provide clear description
4. Respond to feedback

## Support and Questions

### Getting Help
- Check existing documentation
- Review common issues
- Search closed issues
- Ask in discussion forum

### Reporting Issues
- Use issue template
- Provide clear description
- Include steps to reproduce
- Suggest improvements

---

**Maintenance Guide**: Procedures for keeping database schema documentation accurate, current, and useful through regular maintenance and validation.
