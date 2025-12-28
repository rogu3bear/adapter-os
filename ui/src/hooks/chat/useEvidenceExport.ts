/**
 * useEvidenceExport - Handle evidence bundle export for chat messages
 *
 * Provides evidence export functionality with canonical endpoint,
 * legacy fallback, and local bundle generation.
 *
 * @example
 * ```tsx
 * const { exportEvidence } = useEvidenceExport({
 *   tenantId: 'my-tenant',
 *   workspaceActiveState: { activePlanId: 'plan-1', manifestHashB3: 'abc123' },
 * });
 *
 * // Export evidence for a message
 * await exportEvidence(message);
 * ```
 */

import { useCallback } from 'react';
import { apiClient } from '@/api/services';
import { logger, toError } from '@/utils/logger';
import { toast } from 'sonner';
import type { ChatMessage, RunMetadata } from '@/types/components';
import type { WorkspaceActiveState } from './useWorkspaceActiveState';

// ============================================================================
// Types
// ============================================================================

/**
 * Hook configuration options
 */
export interface UseEvidenceExportOptions {
  /** Current tenant ID */
  tenantId: string;
  /** Workspace active state snapshot */
  workspaceActiveState?: WorkspaceActiveState | null;
}

/**
 * Hook return value
 */
export interface UseEvidenceExportReturn {
  /** Export evidence for a chat message */
  exportEvidence: (message: ChatMessage) => Promise<void>;
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * Handle evidence bundle export for chat messages
 *
 * Features:
 * - Canonical endpoint first, then legacy fallback
 * - Local bundle generation if API export fails
 * - Proper file download handling
 */
export function useEvidenceExport(options: UseEvidenceExportOptions): UseEvidenceExportReturn {
  const { tenantId, workspaceActiveState } = options;

  const exportEvidence = useCallback(
    async (message: ChatMessage) => {
      const runMeta = message.runMetadata;
      const runMetaRecord = runMeta as Record<string, unknown> | undefined;
      const readMetaScalar = (keys: string[]): string | number | boolean | undefined => {
        for (const key of keys) {
          const value = runMetaRecord?.[key];
          if (value === undefined || value === null) continue;
          if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
            return value;
          }
        }
        return undefined;
      };

      const rawRunId =
        (runMeta?.runId || runMeta?.requestId || readMetaScalar(['run_id', 'request_id'])) ??
        message.traceId ??
        message.requestId ??
        undefined;
      const runId = rawRunId === undefined ? undefined : String(rawRunId);
      const traceId = message.traceId || runMeta?.traceId || runId || undefined;
      const baseName = runId || traceId || message.id;
      const apiFilename = `run-evidence-${baseName}.zip`;
      const localFilename = `run-evidence-${baseName}-unverified-local-bundle.json`;

      if (!runId && !traceId) {
        toast.error('Evidence is still loading for this run.');
        return;
      }

      const downloadBundle = async (path: string, filename: string, label: 'canonical' | 'legacy'): Promise<boolean> => {
        try {
          const url = apiClient.buildUrl(path);
          const token = apiClient.getToken();
          const response = await fetch(url, {
            method: 'GET',
            headers: token ? { Authorization: `Bearer ${token}` } : undefined,
          });

          if (!response.ok) {
            throw new Error(`Export failed (${response.status})`);
          }

          const blob = await response.blob();
          const blobUrl = window.URL.createObjectURL(blob);
          const link = document.createElement('a');
          link.href = blobUrl;
          link.download = filename;
          document.body.appendChild(link);
          link.click();
          document.body.removeChild(link);
          window.URL.revokeObjectURL(blobUrl);
          return true;
        } catch (err) {
          logger.warn(
            'Evidence export via API failed, falling back',
            {
              component: 'useEvidenceExport',
              endpoint: label,
              path,
              messageId: message.id,
              traceId,
              runId,
            },
            toError(err)
          );
          return false;
        }
      };

      const fallbackExport = () => {
        const toStringOrNull = (value: string | number | boolean | undefined | null): string | null =>
          value === undefined || value === null ? null : String(value);
        const workspaceId = toStringOrNull(
          readMetaScalar(['workspaceId', 'workspace_id', 'tenantId', 'tenant_id']) ?? tenantId
        );
        const routerSeed = toStringOrNull(readMetaScalar(['routerSeed', 'router_seed']));
        const tick = readMetaScalar(['tick']);
        const determinismVersion = toStringOrNull(readMetaScalar(['determinismVersion', 'determinism_version']));
        const bootTraceId = toStringOrNull(readMetaScalar(['bootTraceId', 'boot_trace_id']));
        const createdAt = toStringOrNull(readMetaScalar(['createdAt', 'created_at']));
        const rawReasoningMode = runMeta?.reasoningMode ?? readMetaScalar(['reasoningMode', 'reasoning_mode']);
        const reasoningMode =
          typeof rawReasoningMode === 'boolean'
            ? rawReasoningMode
            : typeof rawReasoningMode === 'string'
              ? rawReasoningMode.toLowerCase() === 'true'
                ? true
                : rawReasoningMode.toLowerCase() === 'false'
                  ? false
                  : null
              : null;

        const bundle = {
          bundle_label: 'unverified local bundle',
          run_id: runId ?? null,
          workspace_id: workspaceId ?? null,
          manifest_hash_b3: runMeta?.manifestHashB3 ?? workspaceActiveState?.manifestHashB3 ?? null,
          policy_mask_digest_b3:
            runMeta?.policyMaskDigestB3 ??
            workspaceActiveState?.policyMaskDigestB3 ??
            message.routerDecision?.policy_mask_digest ??
            null,
          plan_id: runMeta?.planId ?? workspaceActiveState?.activePlanId ?? null,
          router_seed: routerSeed ?? null,
          tick: typeof tick === 'number' || typeof tick === 'string' ? tick : null,
          worker_id: runMeta?.workerId ?? null,
          reasoning_mode: reasoningMode,
          determinism_version: determinismVersion,
          boot_trace_id: bootTraceId,
          created_at: createdAt,
          message_id: message.id,
        };

        const blob = new Blob([JSON.stringify(bundle, null, 2)], { type: 'application/json' });
        const url = window.URL.createObjectURL(blob);
        const link = document.createElement('a');
        link.href = url;
        link.download = localFilename;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        window.URL.revokeObjectURL(url);
      };

      const targetId = runId || traceId;
      if (!targetId) return;
      const canonicalPath = `/v1/runs/${encodeURIComponent(String(targetId))}/evidence`;
      const legacyPath = `/v1/evidence/runs/${encodeURIComponent(String(targetId))}/export`;
      const exportedCanonical = await downloadBundle(canonicalPath, apiFilename, 'canonical');

      if (exportedCanonical) {
        // Note: link.click() is fire-and-forget; we can only confirm the download was initiated
        toast.success('Evidence bundle download started');
        return;
      }

      const exportedLegacy = await downloadBundle(legacyPath, apiFilename, 'legacy');
      if (exportedLegacy) {
        toast.warning('Evidence bundle download started (via legacy endpoint)');
        return;
      }

      fallbackExport();
      toast.warning('API export failed; local bundle download started (unverified)');
    },
    [tenantId, workspaceActiveState?.activePlanId, workspaceActiveState?.manifestHashB3, workspaceActiveState?.policyMaskDigestB3]
  );

  return {
    exportEvidence,
  };
}
