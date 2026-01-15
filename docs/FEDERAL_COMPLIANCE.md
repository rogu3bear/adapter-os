# Federal Compliance Guide

**Document Version:** 1.0
**Last Updated:** 2026-01-14
**Status:** Draft - Requires External Validation
**Maintained by:** adapterOS Security Team

---

## Table of Contents

1. [Overview](#overview)
2. [Compliance Framework Status](#compliance-framework-status)
3. [FedRAMP Readiness](#fedramp-readiness)
4. [ITAR Compliance](#itar-compliance)
5. [FIPS 140-2 Status](#fips-140-2-status)
6. [SOC 2 Type II](#soc-2-type-ii)
7. [Audit Trail Infrastructure](#audit-trail-infrastructure)
8. [Gap Analysis](#gap-analysis)
9. [Roadmap](#roadmap)
10. [External Requirements](#external-requirements)

---

## Overview

This document provides transparency on adapterOS federal compliance status for NSF and other federal agency deployments. It identifies implemented controls, known gaps, and the roadmap for achieving full compliance.

### Compliance Maturity

| Framework | Status | Implementation |
|-----------|--------|----------------|
| FedRAMP | Not Started | Requires JAB authorization |
| ITAR | Partial | Flag exists, enforcement limited |
| FIPS 140-2 | Not Certified | Uses non-FIPS crypto libraries |
| SOC 2 Type II | Not Audited | Controls implemented, no audit |
| ISO 27001 | Not Certified | Controls implemented, no audit |

---

## Compliance Framework Status

### What's Implemented

adapterOS has strong security foundations that map to compliance controls:

- **Authentication**: JWT with Ed25519 signatures, Argon2id password hashing
- **Authorization**: 5-tier RBAC with 56 permissions, tenant isolation
- **Cryptography**: Ed25519, AES-256-GCM, BLAKE3, HKDF-SHA256
- **Audit Logging**: Comprehensive event logging with 10+ audit tables
- **Hardware Security**: Secure Enclave integration on Apple Silicon
- **Access Control**: IP allowlist/denylist, rate limiting
- **Determinism**: HKDF-seeded reproducible inference for audit replay

### What's Missing

- Third-party security audit
- Formal compliance certifications
- FedRAMP authorization package
- FIPS-validated cryptographic modules
- Formal incident response procedures

---

## FedRAMP Readiness

### Current Status: NOT STARTED

FedRAMP authorization requires:
1. **Sponsoring Agency**: Must be sponsored by a federal agency
2. **3PAO Assessment**: Third-party assessment organization audit
3. **JAB Authorization**: Joint Authorization Board review
4. **Continuous Monitoring**: Ongoing compliance validation

### Control Mapping (Preliminary)

| FedRAMP Control Family | adapterOS Coverage |
|------------------------|-------------------|
| AC (Access Control) | Strong - RBAC, tenant isolation |
| AU (Audit) | Strong - Comprehensive logging |
| IA (Identification/Auth) | Strong - JWT, Ed25519 |
| SC (System/Comms) | Partial - TLS required, egress controls |
| SI (System Integrity) | Partial - Determinism validation |
| CM (Config Management) | Partial - No formal CM process |
| IR (Incident Response) | Weak - No formal IRP |
| CP (Contingency Planning) | Weak - No DR procedures |

### FedRAMP Gap Summary

**Critical Gaps:**
- No 3PAO assessment
- No System Security Plan (SSP)
- No continuous monitoring infrastructure
- No Plan of Action and Milestones (POA&M)

---

## ITAR Compliance

### Current Status: PARTIAL IMPLEMENTATION

International Traffic in Arms Regulations (ITAR) controls the export of defense articles and services.

### What's Implemented

```rust
// Tenant model includes ITAR flag
pub struct Tenant {
    pub itar_flag: bool,  // ITAR tracking at tenant level
    // ...
}
```

- ITAR flag stored per tenant
- Flag propagates through SQL and KV backends
- Basic admin-only restrictions for ITAR tenants in journey handlers

### What's NOT Implemented

| ITAR Requirement | Status |
|------------------|--------|
| U.S. Person verification | Not implemented |
| Data residency (US-only) | Not enforced |
| Export control validation | Not implemented |
| ITAR-specific audit events | Not implemented |
| Technical data classification | Not implemented |
| Access logging for ITAR data | Basic only |

### ITAR Enforcement Roadmap

1. **Phase 1** (Planned): Add ITAR middleware with enhanced audit logging
2. **Phase 2** (Planned): Implement geo-blocking for ITAR tenants
3. **Phase 3** (Future): U.S. Person verification workflow
4. **Phase 4** (Future): ITAR data classification and marking

---

## FIPS 140-2 Status

### Current Status: NOT CERTIFIED

FIPS 140-2 specifies security requirements for cryptographic modules used by federal agencies.

### Current Cryptographic Libraries

| Operation | Library | FIPS Status |
|-----------|---------|-------------|
| Hashing | BLAKE3 | Not FIPS-approved |
| Signing | ed25519-dalek | Not FIPS-certified |
| Encryption | aes-gcm | Not FIPS-certified |
| Key Derivation | hkdf | Not FIPS-certified |
| Password Hashing | argon2 | Not FIPS-approved |

### FIPS Compliance Path

To achieve FIPS 140-2 compliance, adapterOS would need:

1. Replace BLAKE3 with SHA-256/SHA-3 for hashing
2. Use FIPS-certified cryptographic module (e.g., AWS-LC, BoringSSL)
3. Replace Argon2id with PBKDF2 for password hashing
4. Obtain CMVP validation for the cryptographic module
5. Document cryptographic boundary and security policy

**Note**: FIPS compliance would impact determinism guarantees (BLAKE3 provides better performance and determinism characteristics than SHA-256).

---

## SOC 2 Type II

### Current Status: CONTROLS IMPLEMENTED, NOT AUDITED

SOC 2 requires audit by a licensed CPA firm over a minimum 6-month period.

### Trust Service Criteria Coverage

| Criteria | Status | Evidence |
|----------|--------|----------|
| Security | Strong | RBAC, encryption, audit logs |
| Availability | Partial | No formal SLA documentation |
| Processing Integrity | Strong | Determinism, receipts |
| Confidentiality | Strong | Tenant isolation, encryption |
| Privacy | Partial | No formal privacy policy |

---

## Audit Trail Infrastructure

### Implemented Audit Tables

- `audit_logs` - Core audit events
- `crypto_audit_logs` - Cryptographic operations
- `policy_audit_decisions` - Policy enforcement
- `model_operations_audit` - Model lifecycle
- `security_compliance` - Compliance events
- `enclave_audit` - Secure Enclave operations
- `lifecycle_audit_enrichment` - Enhanced lifecycle tracking

### Audit Event Types

```rust
pub enum AuditEventType {
    Auth,           // Authentication events
    Authz,          // Authorization decisions
    Adapter,        // Adapter operations
    Training,       // Training jobs
    Policy,         // Policy enforcement
    Tenant,         // Tenant management
    Crypto,         // Cryptographic operations
    System,         // System events
}
```

### Retention Policy

- **Default**: 7 years (2,555 days) per compliance pack
- **Immutability**: Required for compliance tenants
- **Export**: SQL queries provided in SECURITY.md

---

## Gap Analysis

### Critical Gaps (Blocking Federal Deployment)

| Gap | Impact | Remediation |
|-----|--------|-------------|
| No third-party security audit | Cannot validate security claims | Engage audit firm (~$50-100k) |
| No FedRAMP authorization | Cannot deploy to federal cloud | 12-18 month process |
| ITAR enforcement incomplete | ITAR data may leak | Implement middleware |
| No FIPS-certified crypto | May violate federal requirements | Replace crypto libraries |

### High Priority Gaps

| Gap | Impact | Remediation |
|-----|--------|-------------|
| No formal incident response plan | Compliance violation | Document IRP |
| No DR/BC procedures | Availability risk | Document procedures |
| No formal change management | Audit finding | Implement CM process |

---

## Roadmap

### Phase 1: Documentation (Weeks 1-2)
- [x] Create FEDERAL_COMPLIANCE.md (this document)
- [ ] Create INCIDENT_RESPONSE.md
- [ ] Document DR/BC procedures
- [ ] Create formal change management process

### Phase 2: ITAR Enforcement (Weeks 3-4)
- [ ] Implement ITAR middleware
- [ ] Add ITAR-specific audit events
- [ ] Implement geo-blocking (optional)

### Phase 3: External Validation (Weeks 5-16)
- [ ] Engage third-party security audit firm
- [ ] Conduct penetration testing
- [ ] Address audit findings
- [ ] Obtain audit report

### Phase 4: Certification (Future)
- [ ] FedRAMP sponsorship (requires agency partner)
- [ ] SOC 2 Type II audit (6+ months)
- [ ] FIPS 140-2 evaluation (if required)

---

## External Requirements

### What adapterOS Cannot Self-Certify

The following require external resources and cannot be completed internally:

1. **Third-Party Security Audit**
   - Cost: ~$50,000-100,000
   - Timeline: 6-8 weeks
   - Output: Audit report with findings

2. **FedRAMP Authorization**
   - Requires: Federal agency sponsor
   - Cost: ~$500,000-1,000,000+
   - Timeline: 12-18 months
   - Output: ATO (Authority to Operate)

3. **FIPS 140-2 Certification**
   - Requires: NVLAP-accredited laboratory
   - Cost: ~$50,000-200,000
   - Timeline: 6-12 months
   - Output: CMVP certificate

4. **SOC 2 Type II Audit**
   - Requires: Licensed CPA firm
   - Cost: ~$30,000-100,000
   - Timeline: 6+ months observation period
   - Output: SOC 2 report

5. **Legal ITAR/EAR Review**
   - Requires: Export control attorney
   - Cost: ~$10,000-50,000
   - Timeline: 2-4 weeks
   - Output: Legal opinion

---

## Contact

For federal compliance inquiries:
- Security Team: security@adapteros.dev
- Compliance: compliance@adapteros.dev

---

*This document is provided for transparency and planning purposes. It does not constitute legal advice or certification claims.*
