/**
 * generateEvidenceBundle - Creates an evidence bundle with signatures and checksums
 *
 * Generates a structured JSON export containing:
 * - All evidence items with full metadata (bbox, char_range)
 * - Trace IDs and backend references
 * - Signatures for each trace
 * - Bundle checksum for integrity verification
 */

import type {
  EvidenceBundleExport,
  ExtendedEvidenceItem,
  ExtendedMessageExport,
} from './types';

export interface GenerateEvidenceBundleOptions {
  /** Messages containing evidence and trace info */
  messages: ExtendedMessageExport[];
  /** Optional export ID (generated if not provided) */
  exportId?: string;
  /** Backend ID for trace references */
  backendId?: string;
}

/**
 * Generate evidence bundle from chat messages
 *
 * @returns Promise resolving to the evidence bundle with cryptographic checksums
 *
 * @remarks
 * This function is async because it uses Web Crypto API for SHA-256 hashing.
 * The bundle hash provides client-side integrity verification during export.
 *
 * **Security Note**: This uses SHA-256 computed client-side for export integrity
 * checking only. For cryptographic proof and audit trail, the backend uses BLAKE3
 * with Ed25519 signatures. Real signatures must come from the backend's secure
 * enclave, not client-side computation.
 *
 * @example
 * ```typescript
 * const bundle = await generateEvidenceBundle({
 *   messages: chatMessages,
 *   exportId: 'export-123',
 *   backendId: 'aos-worker'
 * });
 * ```
 */
export async function generateEvidenceBundle({
  messages,
  exportId,
  backendId = 'aos-worker',
}: GenerateEvidenceBundleOptions): Promise<EvidenceBundleExport> {
  const id = exportId ?? generateExportId();
  const timestamp = new Date().toISOString();

  // Collect all evidence items
  const allEvidence: ExtendedEvidenceItem[] = [];
  const traces: Array<{ traceId: string; backendId: string }> = [];
  const signatures: Array<{ traceId: string; signature: string; signedAt: string }> = [];

  for (const message of messages) {
    // Collect evidence
    if (message.evidence) {
      for (const item of message.evidence) {
        // Avoid duplicates
        if (!allEvidence.some((e) => e.chunkId === item.chunkId)) {
          allEvidence.push(item);
        }
      }
    }

    // Collect trace info
    if (message.traceId) {
      traces.push({
        traceId: message.traceId,
        backendId,
      });

      // Add signature if verified
      if (message.isVerified && message.verifiedAt) {
        signatures.push({
          traceId: message.traceId,
          signature: message.proofDigest ?? await computeSignature(message.traceId),
          signedAt: message.verifiedAt,
        });
      }
    }
  }

  // Compute bundle hash
  const bundleContent = JSON.stringify({
    traces,
    evidence: allEvidence,
    signatures,
    timestamp,
  });
  const bundleHash = await computeHash(bundleContent);

  return {
    schemaVersion: '1.0.0',
    exportTimestamp: timestamp,
    exportId: id,
    traces,
    evidence: allEvidence,
    signatures,
    checksums: {
      bundleHash,
    },
  };
}

/**
 * Generate a unique export ID
 */
function generateExportId(): string {
  const timestamp = Date.now().toString(36);
  const random = Math.random().toString(36).substring(2, 8);
  return `export-${timestamp}-${random}`;
}

/**
 * Compute SHA-256 hash for bundle integrity verification (client-side)
 *
 * @param content - String content to hash
 * @returns Promise resolving to hex-encoded hash with 0x prefix
 *
 * @remarks
 * Uses Web Crypto API's SHA-256 implementation for client-side integrity checks.
 * This is NOT for cryptographic proof - the backend uses BLAKE3 for that purpose.
 *
 * **Security Considerations**:
 * - Client-side hashing is for export integrity verification only
 * - Backend uses BLAKE3 for deterministic replay and audit trails
 * - Real cryptographic signatures come from backend's Ed25519 signing
 * - This hash can be tampered with since it's computed client-side
 * - For audit purposes, always verify against backend-signed proofs
 *
 * @see {@link https://developer.mozilla.org/en-US/docs/Web/API/SubtleCrypto/digest}
 */
async function computeHash(content: string): Promise<string> {
  const encoder = new TextEncoder();
  const data = encoder.encode(content);
  const hashBuffer = await crypto.subtle.digest('SHA-256', data);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  return '0x' + hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
}

/**
 * Compute a signature placeholder for traces (client-side)
 *
 * @param traceId - Trace identifier to sign
 * @returns Promise resolving to a placeholder signature
 *
 * @remarks
 * This generates a deterministic placeholder signature for traces that don't
 * have backend-verified proof digests. It's meant for export completeness only.
 *
 * **Security Warning**:
 * - This is NOT a cryptographic signature
 * - Real signatures must come from the backend's secure signing process
 * - Backend uses Ed25519 signatures over BLAKE3 digests
 * - This is merely a client-side placeholder for unverified traces
 * - Never trust these signatures for audit or compliance purposes
 */
async function computeSignature(traceId: string): Promise<string> {
  const hash = await computeHash(traceId + Date.now());
  return `sig-${hash.slice(2, 18)}`; // Use first 16 hex chars (64 bits)
}

/**
 * Download evidence bundle as JSON file
 */
export function downloadEvidenceBundle(
  bundle: EvidenceBundleExport,
  filename?: string
): void {
  const json = JSON.stringify(bundle, null, 2);
  const blob = new Blob([json], { type: 'application/json' });
  const url = URL.createObjectURL(blob);

  const a = document.createElement('a');
  a.href = url;
  a.download = filename ?? `evidence-bundle-${bundle.exportId}.json`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

export default generateEvidenceBundle;
