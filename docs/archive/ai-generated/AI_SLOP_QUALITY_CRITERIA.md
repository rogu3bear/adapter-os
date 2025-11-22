# AI Slop Quality Criteria for AdapterOS

**Date:** 2025-11-20
**Purpose:** Define quality standards and AI slop indicators for the AdapterOS codebase
**Domain:** ML inference, deterministic execution, policy enforcement

---

## 🎯 Quality Standards

### **1. Domain-Specific Excellence**

#### ✅ **HIGH QUALITY INDICATORS:**
- **Concrete technical details**: References to specific algorithms (K-sparse routing, Q15 quantization, HKDF seeding)
- **Platform expertise**: Apple Silicon optimizations, Metal kernel specifics, memory management
- **Policy compliance**: References to 23 canonical policy packs with specific enforcement rules
- **Error specificity**: `AosError` variants with domain context (`PolicyViolation`, `DeterminismViolation`)

#### ❌ **AI SLOP INDICATORS:**
- Generic ML terminology without AdapterOS specifics
- Platform-agnostic code that could work anywhere
- Vague references to "policies" or "security" without naming specific packs
- Generic error handling (`anyhow::Error`, `Box<dyn std::error::Error>`)

### **2. Implementation Depth**

#### ✅ **HIGH QUALITY PATTERNS:**
```rust
// Domain-specific with concrete details
pub async fn load_adapter_with_policy(
    &self,
    adapter_id: &str,
    tenant_id: &str,
    required_policies: &[PolicyId],
) -> Result<AdapterHandle, AosError> {
    // Verify tenant isolation
    self.registry.check_acl(adapter_id, tenant_id)
        .map_err(|e| AosError::Isolation(format!(
            "Adapter {} not accessible by tenant {}: {}",
            adapter_id, tenant_id, e
        )))?;

    // Check policy compliance
    for policy in required_policies {
        if !self.policy_engine.check_compliance(adapter_id, *policy).await? {
            return Err(AosError::PolicyViolation(format!(
                "Adapter {} violates policy {:?}", adapter_id, policy
            )));
        }
    }

    // Implementation with safety guarantees
    Ok(self.load_with_memory_checks(adapter_id).await?)
}
```

#### ❌ **AI SLOP PATTERNS:**
```rust
// Generic, could be any system
pub async fn load_data(&self, id: &str, user: &str) -> Result<Data, Error> {
    // Basic validation
    if id.is_empty() {
        return Err(Error::InvalidInput("ID cannot be empty".to_string()));
    }

    // Generic processing
    let result = self.process(id).await
        .map_err(|e| Error::ProcessingError(format!("Failed: {}", e)))?;

    Ok(result)
}
```

### **3. Documentation Quality**

#### ✅ **HIGH QUALITY DOCUMENTATION:**
- Concrete examples with real function signatures and error codes
- Performance benchmarks with specific hardware (M3 Max, 42 tok/s)
- Cross-references to implementation files with line numbers
- Migration guides with actual SQL commands and rollback procedures

#### ❌ **AI SLOP DOCUMENTATION:**
- Generic descriptions applicable to any ML system
- Vague performance claims without benchmarks
- Missing concrete examples or error codes
- Boilerplate API documentation

### **4. Code Structure & Patterns**

#### ✅ **ADAPTEROS-SPECIFIC PATTERNS:**
- HKDF seeding for deterministic randomness
- Policy enforcement with 23 canonical packs
- Content-addressed artifacts with BLAKE3 hashing
- Memory management with ≥15% headroom maintenance
- Audit logging with structured telemetry events

#### ❌ **GENERIC PATTERNS TO WATCH:**
- Standard `rand::thread_rng()` instead of HKDF
- Basic error handling without policy context
- Simple file I/O without content addressing
- Generic logging without structured fields

---

## 🔍 Detection Methodology

### **Automated Pattern Matching:**

1. **Generic Error Detection:**
   ```bash
   grep -r "Failed to" --include="*.rs" src/ | grep -v "AosError::"
   grep -r "Error::" --include="*.rs" src/ | grep -v "AosError::"
   ```

2. **Platform-Agnostic Code:**
   ```bash
   grep -r "std::thread::spawn" --include="*.rs" src/  # Should use deterministic spawn
   grep -r "rand::thread_rng" --include="*.rs" src/   # Should use HKDF
   ```

3. **Missing Context:**
   ```bash
   grep -r "TODO\|FIXME\|XXX" --include="*.rs" src/ | wc -l
   grep -r "// .*generic.*\|// .*placeholder.*" --include="*.rs" src/
   ```

### **Human Review Checklist:**

- [ ] **Domain Knowledge**: Shows understanding of ML inference, deterministic execution?
- [ ] **Policy Awareness**: References specific policy packs and enforcement rules?
- [ ] **Platform Specific**: Optimizes for Apple Silicon, Metal kernels, UMA?
- [ ] **Error Context**: Uses `AosError` with specific variants and context?
- [ ] **Testing Quality**: Tests validate real behavior, not just structure?
- [ ] **Documentation**: Concrete examples, benchmarks, migration guides?

---

## 📊 Quality Metrics

### **Code Quality Score:**
- **Domain Specificity**: 0-10 (Concrete AdapterOS details vs generic patterns)
- **Error Handling**: 0-10 (AosError with context vs generic errors)
- **Documentation**: 0-10 (Concrete examples vs boilerplate)
- **Testing**: 0-10 (Behavioral validation vs structural tests)

### **AI Slop Risk Levels:**
- **🔴 HIGH RISK**: Generic patterns, platform-agnostic code, vague documentation
- **🟡 MEDIUM RISK**: Some domain elements but missing context or testing
- **🟢 LOW RISK**: Specific to AdapterOS domain with proper implementation

---

## 🎯 Action Thresholds

### **Immediate Review Required:**
- Any code with generic error handling
- Files with >3 generic patterns
- Documentation without concrete examples
- Untested public APIs

### **Priority Cleanup Order:**
1. **Security-critical code** (authentication, authorization, policy enforcement)
2. **Core business logic** (inference pipeline, adapter loading)
3. **Public APIs** (REST endpoints, CLI interfaces)
4. **Internal utilities** (data processing, configuration)
5. **Test code** (infrastructure, fixtures)

---

## 📈 Continuous Improvement

### **Monthly Reviews:**
- Sample 10% of modified files for AI slop indicators
- Track quality metrics over time
- Update criteria based on new patterns discovered

### **Prevention Measures:**
- Code review checklists include AI slop detection
- CI/CD includes automated quality checks
- Documentation templates require concrete examples
- Training for contributors on quality standards

---

**Next Step:** Apply these criteria to systematic sampling of the codebase.

