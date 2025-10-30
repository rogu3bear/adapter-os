# Code-Specific Policy Packs

## Overview

Code intelligence adds new policy requirements beyond the existing 22 AdapterOS policy packs. These focus on evidence grounding, patch safety, and preventing code injection attacks.

---

## Code Policy Pack

### Schema

```json
{
  "code": {
    "evidence_min_spans": 1,
    "allow_auto_apply": false,
    "require_test_coverage": 0.8,
    "path_allowlist": ["src/**", "lib/**", "tests/**"],
    "path_denylist": ["**/.env*", "**/secrets/**", "**/*.pem"],
    "allow_external_deps": false,
    "secret_patterns": [
      "(?i)(api[_-]?key|password|secret|token)\\\\s*[:=]\\\\s*['\\\"][^'\\\"]{8,}['\\\"]",
      "(?i)(aws[_-]?access[_-]?key|aws[_-]?secret)",
      "-----BEGIN (RSA |EC |DSA )?PRIVATE KEY-----"
    ],
    "max_patch_size_lines": 500,
    "forbidden_operations": [
      "shell_escape",
      "eval",
      "exec_raw",
      "unsafe_deserialization"
    ],
    "require_review": {
      "database_migrations": true,
      "security_changes": true,
      "config_changes": true
    }
  }
}
```

### Rules

#### 1. Evidence Requirements

**Rule**: Every code suggestion must cite at least one evidence span.

**Evidence types**:
- Code span from symbol index
- Test log span
- Framework documentation span
- Internal API documentation

**Enforcement**:
```rust
pub fn enforce_evidence(response: &Response, policy: &CodePolicy) -> Result<()> {
    let evidence_count = response.trace.evidence.len();
    
    if evidence_count < policy.evidence_min_spans {
        return Err(PolicyViolation::InsufficientEvidence {
            required: policy.evidence_min_spans,
            provided: evidence_count,
        });
    }
    
    Ok(())
}
```

**Refusal format**:
```json
{
  "status": "insufficient_evidence",
  "needed": ["file_path", "symbol", "test_target"],
  "hint": "Provide file path or symbol name for better context"
}
```

---

#### 2. Patch Safety (Path Restrictions)

**Rule**: Patches can only modify files matching allowlist and not matching denylist.

**Allowlist** (glob patterns):
- `src/**`: Source code
- `lib/**`: Libraries
- `tests/**`: Tests
- `docs/**`: Documentation

**Denylist** (glob patterns):
- `**/.env*`: Environment files
- `**/secrets/**`: Secret directories
- `**/*.pem`: Private keys
- `**/*.key`: Key files
- `.github/workflows/**`: CI/CD workflows
- `**/node_modules/**`: Dependencies
- `**/__pycache__/**`: Build artifacts

**Enforcement**:
```rust
pub fn enforce_path_restrictions(
    patch: &PatchSet,
    policy: &CodePolicy,
) -> Result<()> {
    for file in patch.files() {
        let path = Path::new(&file);
        
        // Check denylist first (higher priority)
        for pattern in &policy.path_denylist {
            if glob_match(pattern, path) {
                return Err(PolicyViolation::PathDenied {
                    file: file.clone(),
                    pattern: pattern.clone(),
                });
            }
        }
        
        // Check allowlist
        let allowed = policy.path_allowlist.iter()
            .any(|pattern| glob_match(pattern, path));
        
        if !allowed {
            return Err(PolicyViolation::PathNotAllowed {
                file: file.clone(),
                allowlist: policy.path_allowlist.clone(),
            });
        }
    }
    
    Ok(())
}
```

---

#### 3. Secret Detection

**Rule**: Patches must not introduce secrets or credentials.

**Patterns** (regex):
- API keys: `(?i)(api[_-]?key|password|secret|token)\s*[:=]\s*['"][^'"]{8,}['"]`
- AWS credentials: `(?i)(aws[_-]?access[_-]?key|aws[_-]?secret)`
- Private keys: `-----BEGIN (RSA |EC |DSA )?PRIVATE KEY-----`
- Generic secrets: `(?i)(bearer|token|password)\s*[:=]\s*['"][^'"]{8,}['"]`

**Enforcement**:
```rust
pub fn enforce_no_secrets(
    patch: &PatchSet,
    policy: &CodePolicy,
) -> Result<()> {
    let mut detections = Vec::new();
    
    for hunk in patch.hunks() {
        for pattern_str in &policy.secret_patterns {
            let pattern = Regex::new(pattern_str)?;
            
            for (line_num, line) in hunk.modified.lines().enumerate() {
                if let Some(m) = pattern.find(line) {
                    detections.push(SecretDetection {
                        file: hunk.file.clone(),
                        line: line_num,
                        column: m.start(),
                        pattern: pattern_str.clone(),
                        matched_text: mask_secret(&line[m.range()]),
                        severity: Severity::Critical,
                    });
                }
            }
        }
    }
    
    if !detections.is_empty() {
        return Err(PolicyViolation::SecretsDetected { detections });
    }
    
    Ok(())
}

fn mask_secret(text: &str) -> String {
    // Show first 3 and last 3 chars, mask middle
    if text.len() <= 6 {
        "***".to_string()
    } else {
        format!("{}***{}", &text[..3], &text[text.len()-3..])
    }
}
```

---

#### 4. Forbidden Operations

**Rule**: Code must not contain dangerous operations.

**Forbidden operations**:
- `shell_escape`: Unescaped shell commands
- `eval`: Dynamic code evaluation
- `exec_raw`: Raw exec calls
- `unsafe_deserialization`: Pickle, YAML unsafe loading

**Detection patterns** (language-specific):

Python:
```python
eval(...) | exec(...) | __import__(...) | compile(...)
pickle.loads(...) | yaml.load(..., Loader=yaml.Loader)
subprocess.shell=True | os.system(...)
```

JavaScript/TypeScript:
```javascript
eval(...) | Function(...) | setTimeout(string, ...) | setInterval(string, ...)
child_process.exec(...) | child_process.spawn(..., {shell: true})
```

Rust:
```rust
unsafe { ... } // Without explicit approval
std::process::Command::new("sh").arg("-c")
```

**Enforcement**:
```rust
pub fn enforce_no_forbidden_ops(
    patch: &PatchSet,
    policy: &CodePolicy,
) -> Result<()> {
    let detectors = get_forbidden_op_detectors(policy);
    let mut violations = Vec::new();
    
    for hunk in patch.hunks() {
        let language = detect_language(&hunk.file)?;
        let detector = detectors.get(&language)?;
        
        for (line_num, line) in hunk.modified.lines().enumerate() {
            if let Some(op) = detector.detect(line) {
                violations.push(ForbiddenOpDetection {
                    file: hunk.file.clone(),
                    line: line_num,
                    operation: op.name,
                    code_snippet: line.trim().to_string(),
                    severity: Severity::Critical,
                });
            }
        }
    }
    
    if !violations.is_empty() {
        return Err(PolicyViolation::ForbiddenOperations { violations });
    }
    
    Ok(())
}
```

---

#### 5. Auto-Apply Gates

**Rule**: Auto-apply only when all conditions met.

**Conditions**:
- `allow_auto_apply = true`
- Test coverage ≥ `require_test_coverage` (if set)
- All tests pass
- No linter errors
- All policy checks pass

**Enforcement**:
```rust
pub fn can_auto_apply(
    patch: &PatchSet,
    test_results: &TestResults,
    lint_results: &LintResults,
    policy: &CodePolicy,
) -> Result<bool> {
    // Policy must explicitly allow
    if !policy.allow_auto_apply {
        return Ok(false);
    }
    
    // Tests must pass
    if test_results.failed > 0 {
        return Ok(false);
    }
    
    // Coverage check (if required)
    if let Some(min_coverage) = policy.require_test_coverage {
        let coverage = compute_coverage(patch, test_results)?;
        if coverage < min_coverage {
            return Ok(false);
        }
    }
    
    // No linter errors
    if lint_results.errors > 0 {
        return Ok(false);
    }
    
    // All policy checks pass
    enforce_path_restrictions(patch, policy)?;
    enforce_no_secrets(patch, policy)?;
    enforce_no_forbidden_ops(patch, policy)?;
    
    Ok(true)
}
```

---

#### 6. Patch Size Limits

**Rule**: Patches must not exceed maximum size.

**Default**: 500 lines per patch set

**Rationale**: Large patches are harder to review and more likely to introduce bugs.

**Enforcement**:
```rust
pub fn enforce_patch_size(
    patch: &PatchSet,
    policy: &CodePolicy,
) -> Result<()> {
    let total_lines: usize = patch.hunks()
        .map(|h| h.modified.lines().count())
        .sum();
    
    if total_lines > policy.max_patch_size_lines {
        return Err(PolicyViolation::PatchTooLarge {
            size: total_lines,
            max: policy.max_patch_size_lines,
        });
    }
    
    Ok(())
}
```

---

#### 7. Dependency Policy

**Rule**: External dependencies require approval.

**Default**: `allow_external_deps = false`

**Enforcement**:
```rust
pub fn enforce_dependency_policy(
    patch: &PatchSet,
    policy: &CodePolicy,
) -> Result<()> {
    if policy.allow_external_deps {
        return Ok(());
    }
    
    let new_deps = detect_new_dependencies(patch)?;
    
    if !new_deps.is_empty() {
        return Err(PolicyViolation::UnauthorizedDependencies {
            dependencies: new_deps,
            hint: "Contact admin to request approval",
        });
    }
    
    Ok(())
}
```

---

#### 8. Review Requirements

**Rule**: Certain changes require manual review even if auto-apply is enabled.

**Require review for**:
- Database migrations
- Security-sensitive changes (auth, crypto, permissions)
- Configuration changes (production configs)

**Detection**:
```rust
pub fn requires_review(
    patch: &PatchSet,
    policy: &CodePolicy,
) -> Result<bool> {
    for file in patch.files() {
        // Database migrations
        if policy.require_review.database_migrations {
            if is_migration_file(file) {
                return Ok(true);
            }
        }
        
        // Security changes
        if policy.require_review.security_changes {
            if is_security_file(file) {
                return Ok(true);
            }
        }
        
        // Config changes
        if policy.require_review.config_changes {
            if is_config_file(file) && is_production_config(file) {
                return Ok(true);
            }
        }
    }
    
    Ok(false)
}
```

---

## Enforcement Flow

### Patch Propose

```
1. Evidence check → Refuse if insufficient
2. Path restrictions → Refuse if denied
3. Secret scan → Refuse if found
4. Forbidden ops → Refuse if found
5. Size check → Warn if large
6. Generate patch → With citations
```

### Patch Apply

```
1. Re-run all propose checks
2. Dry-run apply → Create temp worktree
3. Run tests → Count pass/fail
4. Run linter → Check errors
5. Coverage check → If auto-apply enabled
6. Review check → Block auto-apply if required
7. Apply or require manual review
```

---

## Policy Telemetry

All policy enforcements are logged:

```json
{
  "event_type": "policy.code.enforced",
  "tenant_id": "tenant_acme",
  "policy_rule": "evidence_min_spans",
  "result": "pass",
  "details": {
    "evidence_count": 3,
    "min_required": 1
  },
  "timestamp": "2025-10-05T12:00:00Z"
}
```

**Violation events**:
```json
{
  "event_type": "policy.code.violation",
  "tenant_id": "tenant_acme",
  "violation_type": "secret_detected",
  "severity": "critical",
  "patch_set_id": "patch_abc123",
  "file": "config/settings.py",
  "line": 15,
  "timestamp": "2025-10-05T12:00:00Z"
}
```

---

## Configuration Examples

### Permissive (development)

```json
{
  "code": {
    "evidence_min_spans": 1,
    "allow_auto_apply": true,
    "require_test_coverage": 0.7,
    "path_allowlist": ["src/**", "tests/**", "config/**"],
    "path_denylist": ["**/.env*", "**/secrets/**"],
    "allow_external_deps": true,
    "secret_patterns": [
      "(?i)(api[_-]?key|password)\\\\s*=\\\\s*['\\\"][^'\\\"]{8,}['\\\"]"
    ],
    "max_patch_size_lines": 1000,
    "forbidden_operations": ["eval"],
    "require_review": {
      "database_migrations": false,
      "security_changes": false,
      "config_changes": false
    }
  }
}
```

### Strict (production)

```json
{
  "code": {
    "evidence_min_spans": 2,
    "allow_auto_apply": false,
    "require_test_coverage": 0.9,
    "path_allowlist": ["src/**", "tests/**"],
    "path_denylist": [
      "**/.env*",
      "**/secrets/**",
      "**/*.pem",
      "**/*.key",
      ".github/**",
      "**/config/production/**"
    ],
    "allow_external_deps": false,
    "secret_patterns": [
      "(?i)(api[_-]?key|password|secret|token)\\\\s*[:=]\\\\s*['\\\"][^'\\\"]{8,}['\\\"]",
      "(?i)(aws[_-]?access[_-]?key|aws[_-]?secret)",
      "-----BEGIN (RSA |EC |DSA )?PRIVATE KEY-----",
      "(?i)(private[_-]?key|client[_-]?secret)"
    ],
    "max_patch_size_lines": 200,
    "forbidden_operations": [
      "shell_escape",
      "eval",
      "exec_raw",
      "unsafe_deserialization",
      "dynamic_import"
    ],
    "require_review": {
      "database_migrations": true,
      "security_changes": true,
      "config_changes": true
    }
  }
}
```

---

## Integration with Existing Policies

Code policies augment existing AdapterOS policies:

- **Egress policy**: Still deny all network during serving
- **Evidence policy**: Code extends with symbol/test spans
- **Refusal policy**: Code adds structured refusals
- **Memory policy**: Code adapters follow same eviction rules
- **Telemetry policy**: Code events use same sampling

All 22 existing policy packs remain enforced; code policies add an additional layer specific to code modification operations.
