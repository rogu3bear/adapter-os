/**
 * Policy validation and promotion types
 * Extends base types.ts with policy-specific definitions
 */

export type PolicyStatus = 'passed' | 'failed' | 'warning' | 'pending';
export type PolicyCategory = 'security' | 'quality' | 'compliance' | 'performance';
export type PolicySeverity = 'critical' | 'high' | 'medium' | 'low';

/**
 * Individual policy check result
 * Represents a single policy validation against a plan
 */
export interface PolicyCheck {
  // Identity
  id: string;
  name: string;
  description: string;

  // Validation result
  status: PolicyStatus;
  message?: string;

  // Categorization
  category: PolicyCategory;
  severity: PolicySeverity;

  // Remediation
  remediation?: string;
  documentationUrl?: string;

  // Detailed validation info
  details?: PolicyCheckDetails;

  // Override capability
  canOverride?: boolean;
  overrideReason?: string;
}

export interface PolicyCheckDetails {
  expectedValue?: string | number;
  actualValue?: string | number;
  threshold?: string | number;
  componentAffected?: string[];
  errorCode?: string;
  stackTrace?: string;
}

/**
 * Policy check request payload
 */
export interface PolicyCheckRequest {
  cpid: string;
  skipNonCritical?: boolean;
  includeDetails?: boolean;
}

/**
 * Policy check response
 */
export interface PolicyCheckResponse {
  cpid: string;
  policies: PolicyCheck[];
  summary: PolicyCheckSummary;
  checkedAt: string;
}

export interface PolicyCheckSummary {
  total: number;
  passed: number;
  failed: number;
  warnings: number;
  passRate: number;
  canPromote: boolean;
  blockedReasons?: string[];
}

/**
 * Policy override request
 */
export interface PolicyOverrideRequest {
  policyId: string;
  reason: string;
  justification?: string;
  riskAssessment?: string;
  mitigations?: string[];
  approverNotes?: string;
}

/**
 * Policy override response
 */
export interface PolicyOverrideResponse {
  cpid: string;
  policyId: string;
  overriddenBy: string;
  overriddenAt: string;
  reason: string;
  auditId: string;
}

/**
 * Dry run promotion with policy checks
 */
export interface DryRunPromotionWithPoliciesRequest {
  cpid: string;
  includeDetailedChecks?: boolean;
}

export interface DryRunPromotionWithPoliciesResponse {
  cpid: string;
  canPromote: boolean;
  policyCheckResult: PolicyCheckResponse;
  otherGates?: PromotionGateResult[];
  simulatedAt: string;
}

export interface PromotionGateResult {
  name: string;
  status: 'passed' | 'failed' | 'skipped';
  message?: string;
  details?: Record<string, any>;
}

/**
 * Policy categories with their metadata
 */
export const POLICY_CATEGORIES: Record<PolicyCategory, { label: string; description: string }> = {
  security: {
    label: 'Security Policies',
    description: 'Network isolation, data protection, and access control policies',
  },
  quality: {
    label: 'Quality Policies',
    description: 'Determinism, performance, and reliability policies',
  },
  compliance: {
    label: 'Compliance Policies',
    description: 'Audit, evidence tracking, and regulatory compliance policies',
  },
  performance: {
    label: 'Performance Policies',
    description: 'Latency, throughput, and resource utilization policies',
  },
};

/**
 * 23 canonical policies from CLAUDE.md
 * This represents the full policy pack
 */
export const CANONICAL_POLICIES = [
  // Security (Egress)
  {
    id: 'egress',
    name: 'Egress Control',
    category: 'security' as PolicyCategory,
    description: 'Zero network egress in production (UDS-only)',
  },

  // Quality (Determinism)
  {
    id: 'determinism',
    name: 'Determinism',
    category: 'quality' as PolicyCategory,
    description: 'All randomness seeded via HKDF (no thread_rng)',
  },

  // Quality (Router)
  {
    id: 'router',
    name: 'Router Policy',
    category: 'quality' as PolicyCategory,
    description: 'K-sparse LoRA routing with Q15 quantized gates',
  },

  // Compliance (Evidence)
  {
    id: 'evidence',
    name: 'Evidence Tracking',
    category: 'compliance' as PolicyCategory,
    description: 'Audit trail with quality thresholds (min relevance/confidence)',
  },

  // Compliance (Telemetry)
  {
    id: 'telemetry',
    name: 'Telemetry',
    category: 'compliance' as PolicyCategory,
    description: 'Structured event logging with canonical JSON signatures',
  },

  // Compliance (Naming)
  {
    id: 'naming',
    name: 'Semantic Naming',
    category: 'compliance' as PolicyCategory,
    description: 'Adapter naming: {tenant}/{domain}/{purpose}/{revision}',
  },

  // Security (Input Validation)
  {
    id: 'input-validation',
    name: 'Input Validation',
    category: 'security' as PolicyCategory,
    description: 'Validate all inputs for type and format safety',
  },

  // Security (Tenant Isolation)
  {
    id: 'tenant-isolation',
    name: 'Tenant Isolation',
    category: 'security' as PolicyCategory,
    description: 'Enforce tenant data and process isolation',
  },

  // Quality (Typed Errors)
  {
    id: 'typed-errors',
    name: 'Typed Error Handling',
    category: 'quality' as PolicyCategory,
    description: 'Use Result<T> with typed AosError variants',
  },

  // Performance
  {
    id: 'latency-p95',
    name: 'Latency P95',
    category: 'performance' as PolicyCategory,
    description: 'Ensure p95 latency meets threshold',
  },

  // Performance
  {
    id: 'throughput',
    name: 'Throughput',
    category: 'performance' as PolicyCategory,
    description: 'Maintain minimum tokens per second throughput',
  },

  // Security
  {
    id: 'memory-safety',
    name: 'Memory Safety',
    category: 'security' as PolicyCategory,
    description: 'No unsafe blocks in app crates',
  },

  // Quality
  {
    id: 'kernel-hash',
    name: 'Kernel Hash Match',
    category: 'quality' as PolicyCategory,
    description: 'Verify precompiled kernel hash consistency',
  },

  // Compliance
  {
    id: 'audit-logging',
    name: 'Audit Logging',
    category: 'compliance' as PolicyCategory,
    description: 'Log all access and modifications with audit trail',
  },

  // Security
  {
    id: 'artifact-signature',
    name: 'Artifact Signature',
    category: 'security' as PolicyCategory,
    description: 'Verify signatures on all artifacts (ED25519)',
  },

  // Quality
  {
    id: 'lifecycle-state',
    name: 'Lifecycle State',
    category: 'quality' as PolicyCategory,
    description: 'Validate adapter lifecycle transitions',
  },

  // Performance
  {
    id: 'memory-headroom',
    name: 'Memory Headroom',
    category: 'performance' as PolicyCategory,
    description: 'Maintain >= 15% memory headroom',
  },

  // Compliance
  {
    id: 'data-retention',
    name: 'Data Retention',
    category: 'compliance' as PolicyCategory,
    description: 'Enforce data retention and purge policies',
  },

  // Security
  {
    id: 'secrets-rotation',
    name: 'Secrets Rotation',
    category: 'security' as PolicyCategory,
    description: 'Rotate secrets on promotion',
  },

  // Quality
  {
    id: 'adapter-quality',
    name: 'Adapter Quality',
    category: 'quality' as PolicyCategory,
    description: 'Min activation % and quality delta thresholds',
  },

  // Compliance
  {
    id: 'itar-compliance',
    name: 'ITAR Compliance',
    category: 'compliance' as PolicyCategory,
    description: 'Enforce ITAR-compliant artifact handling',
  },

  // Security
  {
    id: 'rate-limiting',
    name: 'Rate Limiting',
    category: 'security' as PolicyCategory,
    description: 'Enforce rate limits on inference endpoints',
  },

  // Compliance
  {
    id: 'control-matrix',
    name: 'Control Matrix',
    category: 'compliance' as PolicyCategory,
    description: 'Verify compliance control coverage',
  },
];

/**
 * Policy preflight check request for adapter operations
 * Used before loading/unloading adapters to validate policy compliance
 */
export interface PolicyPreflightRequest {
  adapterId: string;
  operation: 'load' | 'unload' | 'activate' | 'deactivate';
  includeDetails?: boolean;
}

/**
 * Policy preflight check response
 * Maps to PolicyPreflightDialog component's PolicyCheck interface
 */
export interface PolicyPreflightResponse {
  adapterId: string;
  operation: string;
  canProceed: boolean;
  checks: PolicyPreflightCheck[];
  checkedAt: string;
}

/**
 * Individual preflight check result
 * Compatible with PolicyPreflightDialog component
 */
export interface PolicyPreflightCheck {
  policy_id: string;
  policy_name: string;
  passed: boolean;
  severity: 'error' | 'warning' | 'info';
  message: string;
  can_override?: boolean;
  details?: string;
}

/**
 * Get policy metadata by ID
 */
export function getPolicyMetadata(policyId: string) {
  return CANONICAL_POLICIES.find(p => p.id === policyId);
}

/**
 * Format policy response for UI display
 */
export function formatPolicyCheckForDisplay(check: PolicyCheck): PolicyCheck {
  return {
    ...check,
    message: check.message || getDefaultMessage(check.status),
    remediation: check.remediation || getDefaultRemediation(check.id),
  };
}

function getDefaultMessage(status: PolicyStatus): string {
  switch (status) {
    case 'passed':
      return 'Policy validation passed successfully';
    case 'failed':
      return 'Policy validation failed - blocking promotion';
    case 'warning':
      return 'Policy validation warning - review recommended';
    case 'pending':
      return 'Policy validation in progress';
    default:
      return 'Unknown policy status';
  }
}

function getDefaultRemediation(policyId: string): string {
  const remediations: Record<string, string> = {
    egress: 'Configure UDS socket for production mode and disable TCP/UDP',
    determinism:
      'Replace rand::thread_rng() with HKDF-seeded randomness from the deterministic executor',
    router: 'Verify Q15 quantization is applied to all gate values',
    evidence: 'Ensure all evidence spans have min relevance/confidence scores',
    telemetry: 'Use canonical JSON format with proper event signatures',
    naming: 'Follow {tenant}/{domain}/{purpose}/{revision} naming convention',
    'input-validation': 'Add input validation before processing user data',
    'tenant-isolation': 'Use tenant ID for all data access control checks',
    'typed-errors': 'Replace Option<T> with Result<T, AosError>',
    'latency-p95': 'Profile and optimize slow code paths',
    'throughput': 'Reduce memory allocations or batch process requests',
    'memory-safety': 'Refactor unsafe code to safe alternatives',
    'kernel-hash': 'Verify kernel compilation and hash consistency',
    'audit-logging': 'Add audit log entries for all privileged operations',
    'artifact-signature': 'Sign artifacts with ED25519 key',
    'lifecycle-state': 'Validate state transitions match allowed transitions',
    'memory-headroom': 'Evict adapters to maintain >= 15% headroom',
    'data-retention': 'Configure retention policies and purge schedule',
    'secrets-rotation': 'Enable automatic secrets rotation on promotion',
    'adapter-quality': 'Train adapter with sufficient data or increase threshold',
    'itar-compliance': 'Use ITAR-approved artifact handling procedures',
    'rate-limiting': 'Configure rate limits per tenant/endpoint',
    'control-matrix': 'Map implementation to control requirements',
  };

  return remediations[policyId] || 'Review policy documentation for remediation steps';
}
