export type StatusSignatureAlgorithm = 'digest-sha256';

export interface StatusSignature {
  algorithm: StatusSignatureAlgorithm;
  value: string;
  keyId: string;
  issuedAt: string;
}

export interface StatusTenantRecord {
  tenantId: string;
  displayName: string;
  isolationLevel: string;
  permissions: string[];
  labels?: Record<string, string>;
}

export type OperationState = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';

export interface StatusOperationRecord {
  opId: string;
  tenantId: string;
  command: string;
  state: OperationState;
  retries: number;
  lastUpdated: string;
  metadata?: Record<string, string>;
}

export interface StatusV2 {
  schema: 'status.v2';
  version: 2;
  issuedAt: string;
  expiresAt?: string;
  nonce: string;
  tenants: StatusTenantRecord[];
  operations: StatusOperationRecord[];
  metadata?: Record<string, unknown>;
  signature: StatusSignature;
}

export interface StatusDigestResult {
  canonical: string;
  digest: string;
}

const textEncoder = new TextEncoder();

function canonicalize(value: unknown): string {
  if (value === null || value === undefined) {
    return 'null';
  }
  if (typeof value !== 'object') {
    if (typeof value === 'number' && !Number.isFinite(value)) {
      throw new Error('Non-finite numbers are not supported in canonical JSON');
    }
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    return `[${value.map(item => canonicalize(item)).join(',')}]`;
  }
  const entries = Object.entries(value as Record<string, unknown>)
    .filter(([, v]) => v !== undefined)
    .sort(([a], [b]) => a.localeCompare(b));
  const serialized = entries.map(([key, val]) => `${JSON.stringify(key)}:${canonicalize(val)}`);
  return `{${serialized.join(',')}}`;
}

interface NodeBuffer {
  from(buffer: ArrayBuffer): { toString(encoding: string): string };
}

function arrayBufferToBase64(buffer: ArrayBuffer): string {
  const scope = globalThis as typeof globalThis & { Buffer?: NodeBuffer };
  if (typeof scope.btoa === 'function') {
    const bytes = new Uint8Array(buffer);
    let binary = '';
    for (let i = 0; i < bytes.byteLength; i += 1) {
      binary += String.fromCharCode(bytes[i]);
    }
    return scope.btoa(binary);
  }
  if (typeof scope.Buffer !== 'undefined') {
    return scope.Buffer.from(buffer).toString('base64');
  }
  throw new Error('Unable to encode digest: missing base64 implementation');
}

function timingSafeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) {
    return false;
  }
  let diff = 0;
  for (let i = 0; i < a.length; i += 1) {
    diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return diff === 0;
}

export async function computeStatusDigest(status: StatusV2): Promise<StatusDigestResult> {
  const { signature, ...unsigned } = status;
  const canonical = canonicalize(unsigned);
  const data = textEncoder.encode(canonical);
  const hash = await crypto.subtle.digest('SHA-256', data);
  return {
    canonical,
    digest: arrayBufferToBase64(hash),
  };
}

export interface SignatureVerificationResult {
  valid: boolean;
  expectedDigest: string;
  actualDigest: string;
  algorithm: StatusSignatureAlgorithm;
}

export async function verifyStatusSignature(status: StatusV2): Promise<SignatureVerificationResult> {
  const digestResult = await computeStatusDigest(status);
  const algorithm = status.signature.algorithm;
  if (algorithm !== 'digest-sha256') {
    throw new Error(`Unsupported status signature algorithm: ${algorithm}`);
  }
  const actualDigest = status.signature.value;
  return {
    valid: timingSafeEqual(actualDigest, digestResult.digest),
    expectedDigest: digestResult.digest,
    actualDigest,
    algorithm,
  };
}

export function sanitizeStatus(status: StatusV2): StatusV2 {
  return {
    ...status,
    tenants: status.tenants.map(tenant => ({
      ...tenant,
      permissions: [...tenant.permissions],
      labels: tenant.labels ? { ...tenant.labels } : undefined,
    })),
    operations: status.operations.map(op => ({
      ...op,
      metadata: op.metadata ? { ...op.metadata } : undefined,
    })),
    metadata: status.metadata ? { ...status.metadata } : undefined,
    signature: { ...status.signature },
  };
}

