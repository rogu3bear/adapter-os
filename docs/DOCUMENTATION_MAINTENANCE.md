# Documentation Maintenance Guide

**Processes and tools to keep AdapterOS documentation synchronized with code changes.**

---

## Table of Contents

- [Overview](#overview)
- [API Documentation Sync](#api-documentation-sync)
- [Code Documentation Standards](#code-documentation-standards)
- [Documentation CI/CD](#documentation-cicd)
- [Review Process](#review-process)
- [Breaking Changes](#breaking-changes)
- [Tools and Automation](#tools-and-automation)

---

## Overview

Documentation maintenance is critical for AdapterOS adoption and developer experience. This guide establishes processes to ensure documentation stays synchronized with code changes.

### Key Principles

1. **Documentation is Code** - Documentation changes require the same review process as code changes
2. **Single Source of Truth** - API documentation should be generated from code, not manually maintained
3. **Continuous Verification** - Automated checks ensure documentation accuracy
4. **Developer Responsibility** - Contributors update documentation as part of feature development

---

## API Documentation Sync

### OpenAPI Specification

The OpenAPI spec in `docs/api.md` must be kept in sync with actual API endpoints.

#### Process for Adding New Endpoints

1. **Implement the endpoint** in `crates/adapteros-server-api/src/`
2. **Add OpenAPI annotations** using `utoipa` macros:
   ```rust
   #[cfg_attr(feature = "openapi", utoipa::path(
       post,
       path = "/v1/new/endpoint",
       responses(
           (status = 200, description = "Success response")
       ),
       tag = "new_feature"
   ))]
   pub async fn new_endpoint(...) { ... }
   ```

3. **Add the route** in `crates/adapteros-server-api/src/routes.rs`
4. **Generate updated spec**:
   ```bash
   cargo run -p adapteros-server -- --generate-openapi > temp_openapi.json
   # Manually update docs/api.md with the new spec
   ```

5. **Add comprehensive examples** in the API examples section
6. **Update integration guides** (Python, JS, Go clients)

#### Process for Modifying Endpoints

1. **Update the handler code**
2. **Update OpenAPI annotations** if request/response schemas change
3. **Regenerate and update the spec**
4. **Update examples** to reflect new behavior
5. **Update client libraries** if breaking changes

#### Process for Removing Endpoints

1. **Mark endpoint as deprecated** in OpenAPI spec first
2. **Add deprecation notice** in documentation
3. **Wait for deprecation period** (minimum 2 releases)
4. **Remove endpoint and documentation**

### Verification Checklist

Before merging API changes:

- [ ] OpenAPI spec includes new/modified endpoints
- [ ] Request/response schemas are accurate
- [ ] Authentication requirements are documented
- [ ] Error responses are documented
- [ ] Examples work with actual API
- [ ] Integration examples are updated

---

## Code Documentation Standards

### Rust Documentation

All public APIs must have comprehensive documentation:

```rust
/// Manages adapter lifecycle including loading, unloading, and hot-swapping.
///
/// This manager provides thread-safe operations for adapter management with
/// automatic memory management and performance optimization.
///
/// # Examples
///
/// ```rust
/// use adapteros_lora_lifecycle::LifecycleManager;
///
/// let manager = LifecycleManager::new(adapters, policies, root_path);
/// manager.load_adapter("my_adapter", tenant_id).await?;
/// ```
///
/// # Thread Safety
///
/// All operations are thread-safe and can be called concurrently.
///
/// # Error Handling
///
/// Returns `AdapterError` for all error conditions with detailed error messages.
#[derive(Debug)]
pub struct LifecycleManager { ... }
```

#### Documentation Requirements

- [ ] Public structs, enums, functions have `///` documentation
- [ ] Complex types include usage examples
- [ ] Error conditions are documented
- [ ] Thread safety guarantees are stated
- [ ] Performance characteristics are noted
- [ ] Citations to related documentation

### Code Comments

```rust
// ✅ GOOD: Explains why, not what
// Use LRU eviction to maintain recency bias in adapter selection
let evicted = self.lru_cache.evict(candidates);

// ❌ BAD: Just repeats the code
// Evict candidates from LRU cache
let evicted = self.lru_cache.evict(candidates);
```

#### Comment Standards

- [ ] Explain "why" not "what"
- [ ] Document complex algorithms
- [ ] Note performance trade-offs
- [ ] Reference related issues/PRs
- [ ] Update comments when code changes

---

## Documentation CI/CD

### Automated Checks

#### Pre-commit Hooks

Add to `.pre-commit-config.yaml`:

```yaml
repos:
  - repo: local
    hooks:
      - id: check-api-docs
        name: Check API documentation sync
        entry: ./scripts/check_api_docs.py
        language: system
        files: ^(crates/adapteros-server-api/|docs/api\.md)$
        pass_filenames: false

  - repo: local
    hooks:
      - id: validate-openapi
        name: Validate OpenAPI specification
        entry: ./scripts/validate_openapi.py
        language: system
        files: docs/api\.md
        pass_filenames: false
```

#### GitHub Actions

Add to `.github/workflows/ci.yml`:

```yaml
- name: Check Documentation
  run: |
    # Verify OpenAPI spec is valid JSON
    python3 -c "import json; json.load(open('docs/api.md').split('```json')[1].split('```')[0])"

    # Check that documented endpoints exist in code
    ./scripts/verify_endpoints.py

    # Validate examples can be parsed
    ./scripts/validate_examples.py
```

### Documentation Generation

#### Automated OpenAPI Updates

Create a script `scripts/update_openapi_docs.py`:

```python
#!/usr/bin/env python3
"""Update API documentation from code-generated OpenAPI spec."""

import json
import subprocess
import sys
from pathlib import Path

def update_openapi_docs():
    # Generate fresh OpenAPI spec
    result = subprocess.run([
        'cargo', 'run', '-p', 'adapteros-server', '--', '--generate-openapi'
    ], capture_output=True, text=True, cwd=Path(__file__).parent.parent)

    if result.returncode != 0:
        print("Failed to generate OpenAPI spec:", result.stderr)
        sys.exit(1)

    # Parse the generated spec
    try:
        spec = json.loads(result.stdout)
    except json.JSONDecodeError as e:
        print(f"Invalid JSON from OpenAPI generation: {e}")
        sys.exit(1)

    # Read current docs
    docs_path = Path("docs/api.md")
    content = docs_path.read_text()

    # Replace the OpenAPI spec section
    start_marker = "```json\n"
    end_marker = "\n```"

    start_idx = content.find(start_marker)
    if start_idx == -1:
        print("Could not find OpenAPI spec section in docs")
        sys.exit(1)

    end_idx = content.find(end_marker, start_idx + len(start_marker))
    if end_idx == -1:
        print("Could not find end of OpenAPI spec section")
        sys.exit(1)

    new_spec = json.dumps(spec, indent=2)
    new_content = (
        content[:start_idx + len(start_marker)] +
        new_spec +
        content[end_idx:]
    )

    # Write updated docs
    docs_path.write_text(new_content)
    print("Updated OpenAPI specification in docs/api.md")

if __name__ == "__main__":
    update_openapi_docs()
```

---

## Review Process

### Documentation Review Checklist

**For Code Reviewers:**

- [ ] API documentation is updated for endpoint changes
- [ ] OpenAPI spec includes all new endpoints
- [ ] Examples are provided and tested
- [ ] Error responses are documented
- [ ] Breaking changes are clearly marked
- [ ] Integration guides are updated

**For Documentation Reviewers:**

- [ ] Technical accuracy verified
- [ ] Examples are complete and correct
- [ ] Language is clear and accessible
- [ ] Cross-references are valid
- [ ] Formatting is consistent

### Pull Request Template Updates

Add to `.github/PULL_REQUEST_TEMPLATE.md`:

```markdown
## Documentation Changes

- [ ] Updated API documentation for endpoint changes
- [ ] Added examples for new functionality
- [ ] Updated integration guides
- [ ] Breaking changes documented
- [ ] OpenAPI spec regenerated

## Testing

- [ ] Examples tested against running API
- [ ] Documentation builds successfully
- [ ] Cross-references are valid
```

---

## Breaking Changes

### Documentation Requirements

When making breaking API changes:

1. **Immediate Documentation**
   - Mark old endpoints as deprecated
   - Document migration path
   - Provide timeline for removal

2. **Migration Guide**
   ```markdown
   ## Breaking Changes in v2.0.0

   ### Adapter Registration API

   **Breaking:** The `/v1/adapters/register` endpoint now requires a `manifest` field.

   **Migration:**
   ```diff
   - POST /v1/adapters/register
     {"name": "my-adapter", "rank": 16}

   + POST /v1/adapters/register
     {"manifest": {"name": "my-adapter", "rank": 16, "base_model": "qwen2.5-7b"}}
   ```

   **Timeline:** Old format supported until v3.0.0 (6 months)
   ```

3. **Version Communication**
   - Update CHANGELOG.md
   - Send migration notices to users
   - Provide upgrade scripts if needed

### Deprecation Process

1. **Phase 1: Deprecation Notice** (Release N)
   - Add `deprecated: true` to OpenAPI spec
   - Add deprecation warnings in API responses
   - Update documentation with migration guide

2. **Phase 2: Removal Warning** (Release N+1)
   - API returns 410 Gone for deprecated endpoints
   - Documentation marks endpoints as removed

3. **Phase 3: Complete Removal** (Release N+2)
   - Remove endpoint code
   - Remove from documentation
   - Update examples

---

## Tools and Automation

### Documentation Validation Scripts

#### `scripts/verify_endpoints.py`

```python
#!/usr/bin/env python3
"""Verify that documented API endpoints exist in code."""

import re
from pathlib import Path

def extract_documented_endpoints():
    """Extract endpoints from docs/api.md"""
    docs_path = Path("docs/api.md")
    content = docs_path.read_text()

    # Find OpenAPI spec section
    start = content.find('```json')
    end = content.find('```', start + 1)
    spec_text = content[start:end].replace('```json', '').strip()

    import json
    spec = json.loads(spec_text)

    endpoints = []
    for path, methods in spec.get('paths', {}).items():
        for method in methods.keys():
            endpoints.append(f"{method.upper()} {path}")

    return set(endpoints)

def extract_code_endpoints():
    """Extract endpoints from routes.rs"""
    routes_path = Path("crates/adapteros-server-api/src/routes.rs")
    content = routes_path.read_text()

    endpoints = set()
    # Find .route("path", method(handler)) patterns
    route_pattern = r'\.route\("([^"]+)",\s*(\w+)\('
    matches = re.findall(route_pattern, content)

    for path, method in matches:
        # Convert axum method names to HTTP methods
        method_map = {
            'get': 'GET',
            'post': 'POST',
            'put': 'PUT',
            'delete': 'DELETE',
            'patch': 'PATCH'
        }
        http_method = method_map.get(method.lower(), method.upper())
        endpoints.add(f"{http_method} {path}")

    return endpoints

def main():
    documented = extract_documented_endpoints()
    implemented = extract_code_endpoints()

    missing_docs = implemented - documented
    extra_docs = documented - implemented

    if missing_docs:
        print("❌ Endpoints implemented but not documented:")
        for endpoint in sorted(missing_docs):
            print(f"  {endpoint}")

    if extra_docs:
        print("❌ Endpoints documented but not implemented:")
        for endpoint in sorted(extra_docs):
            print(f"  {endpoint}")

    if not missing_docs and not extra_docs:
        print("✅ All endpoints are properly documented!")
        return 0

    return 1

if __name__ == "__main__":
    exit(main())
```

#### `scripts/validate_examples.py`

```python
#!/usr/bin/env python3
"""Validate that API examples are syntactically correct."""

import json
import re
from pathlib import Path

def validate_json_examples():
    """Check that JSON examples in docs are valid."""
    docs_path = Path("docs/api.md")
    content = docs_path.read_text()

    errors = []

    # Find all JSON code blocks
    json_blocks = re.findall(r'```json\n(.*?)\n```', content, re.DOTALL)

    for i, block in enumerate(json_blocks):
        try:
            # Remove comment markers (#)
            clean_block = '\n'.join(
                line for line in block.split('\n')
                if not line.strip().startswith('#')
            )
            if clean_block.strip():
                json.loads(clean_block)
        except json.JSONDecodeError as e:
            errors.append(f"Invalid JSON in block {i+1}: {e}")

    return errors

def validate_curl_examples():
    """Check that curl examples have proper syntax."""
    docs_path = Path("docs/api.md")
    content = docs_path.read_text()

    errors = []

    # Find curl commands
    curl_commands = re.findall(r'```bash\n(.*?)\n```', content, re.DOTALL)

    for i, command in enumerate(curl_commands):
        if 'curl' in command:
            # Basic validation - has URL
            if 'http' not in command:
                errors.append(f"Curl command {i+1} missing URL")

    return errors

def main():
    json_errors = validate_json_examples()
    curl_errors = validate_curl_examples()

    all_errors = json_errors + curl_errors

    if all_errors:
        print("❌ Documentation validation errors:")
        for error in all_errors:
            print(f"  {error}")
        return 1

    print("✅ All documentation examples are valid!")
    return 0

if __name__ == "__main__":
    exit(main())
```

### Integration Testing

#### API Documentation Tests

Add to `tests/documentation.rs`:

```rust
#[cfg(test)]
mod documentation_tests {
    use std::collections::HashSet;

    #[test]
    fn api_endpoints_match_documentation() {
        // This would use the scripts above to verify sync
        // In real implementation, this would be a build-time check
    }

    #[test]
    fn openapi_spec_is_valid() {
        // Parse and validate the OpenAPI spec
    }

    #[test]
    fn examples_are_syntactically_correct() {
        // Validate JSON and curl examples
    }
}
```

---

## Maintenance Schedule

### Weekly Tasks

- [ ] Review open PRs for documentation requirements
- [ ] Run documentation validation scripts
- [ ] Update any outdated examples

### Monthly Tasks

- [ ] Audit API documentation completeness
- [ ] Review and update integration guides
- [ ] Check for broken cross-references

### Quarterly Tasks

- [ ] Complete documentation audit
- [ ] Update style guides and standards
- [ ] Review and improve automation tools

---

## Getting Help

### When Documentation is Out of Sync

1. **Run validation scripts** to identify issues
2. **Check recent changes** in API code
3. **Regenerate OpenAPI spec** if needed
4. **Update examples** to match current API
5. **Create documentation PR**

### Resources

- **API Documentation**: `docs/api.md`
- **OpenAPI Spec**: Generated from code
- **Validation Scripts**: `scripts/` directory
- **CI/CD**: `.github/workflows/`

---

**Last Updated:** 2025-01-15
**Maintained By:** AdapterOS Documentation Team

This guide ensures AdapterOS documentation remains accurate, complete, and synchronized with code changes.
