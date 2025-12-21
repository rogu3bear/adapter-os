import type {
  AdapterHealthDomain,
  AdapterHealthFlag,
  AdapterHealthResponse,
  AdapterHealthSubcode,
} from '@/api/adapter-types';

const DOMAIN_PRIORITY: Record<AdapterHealthFlag, AdapterHealthDomain[]> = {
  corrupt: ['storage', 'trust', 'drift', 'other'],
  unsafe: ['trust', 'storage', 'drift', 'other'],
  degraded: ['drift', 'trust', 'storage', 'other'],
  healthy: ['drift', 'trust', 'storage', 'other'],
};

const SUBCODE_LABELS: Record<string, string> = {
  'trust:trust_blocked': 'Trust blocked',
  'drift:drift_high': 'Drift above threshold',
  'storage:hash_mismatch': 'Artifact hash mismatch',
  'storage:missing_bytes': 'Artifact missing',
  'storage:missing_file': 'Artifact missing',
  'storage:orphan_bytes': 'Orphaned artifact',
  'storage:orphan_file': 'Orphaned artifact',
};

const SUBCODE_DETAILS: Record<string, string> = {
  'trust:trust_blocked': 'Dataset trust is blocked or regressed',
  'drift:drift_high': 'Drift exceeded the configured hard threshold',
  'storage:hash_mismatch': 'Stored artifact hash does not match the manifest hash',
  'storage:missing_bytes': 'Artifact bytes are missing from storage',
  'storage:missing_file': 'Artifact file is missing from storage',
  'storage:orphan_bytes': 'Artifact bytes exist without matching metadata',
  'storage:orphan_file': 'Artifact file exists without matching metadata',
};

const makeKey = (subcode: AdapterHealthSubcode) => `${subcode.domain}:${subcode.code}`;

export function pickPrimarySubcode(
  health?: AdapterHealthResponse | null
): AdapterHealthSubcode | undefined {
  if (!health || !health.subcodes?.length) return undefined;
  if (health.primary_subcode) return health.primary_subcode;

  const priority = DOMAIN_PRIORITY[health.health] ?? DOMAIN_PRIORITY.corrupt;
  for (const domain of priority) {
    const match = health.subcodes.find(sub => sub.domain === domain);
    if (match) return match;
  }

  return health.subcodes[0];
}

export function describeSubcode(subcode: AdapterHealthSubcode): { label: string; detail: string } {
  const key = makeKey(subcode);
  const label =
    SUBCODE_LABELS[key] ?? subcode.message ?? subcode.code.replace(/_/g, ' ');
  const detail =
    subcode.message ??
    SUBCODE_DETAILS[key] ??
    SUBCODE_LABELS[key] ??
    subcode.code.replace(/_/g, ' ');

  return { label, detail };
}
