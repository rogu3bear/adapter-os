/**
 * useRunMetadataHandler - Handle run metadata merging for chat messages
 *
 * Provides utilities to merge run metadata from streaming responses
 * into existing messages while preserving workspace state.
 *
 * @example
 * ```tsx
 * const { mergeRunMetadata, handleRunMetadata } = useRunMetadataHandler({
 *   setMessages,
 *   streamingMessageId,
 *   workspaceActiveState,
 *   onTraceIdUpdate: (traceId) => setLatestTraceId(traceId),
 * });
 * ```
 */

import { useCallback } from 'react';
import type { ChatMessage, RunMetadata } from '@/types/components';
import type { WorkspaceActiveState } from './useWorkspaceActiveState';

// ============================================================================
// Types
// ============================================================================

/**
 * Hook configuration options
 */
export interface UseRunMetadataHandlerOptions {
  /** State setter for messages */
  setMessages: React.Dispatch<React.SetStateAction<ChatMessage[]>>;
  /** Current streaming message ID */
  streamingMessageId: string | null;
  /** Workspace active state snapshot */
  workspaceActiveState?: WorkspaceActiveState | null;
  /** Callback when trace ID is updated */
  onTraceIdUpdate?: (traceId: string) => void;
}

/**
 * Hook return value
 */
export interface UseRunMetadataHandlerReturn {
  /** Merge run metadata into messages */
  mergeRunMetadata: (metadata: RunMetadata) => void;
  /** Handle run metadata from streaming (with trace ID update) */
  handleRunMetadata: (metadata: RunMetadata) => void;
}

// ============================================================================
// Helpers
// ============================================================================

/**
 * Merge defined fields from update into base object.
 * Preserves existing values when update provides null/undefined.
 * This prevents later partial metadata from overwriting authoritative fields.
 */
const mergeDefinedFields = (base: Record<string, unknown>, update: Record<string, unknown>) => {
  const merged = { ...base };
  for (const [key, value] of Object.entries(update)) {
    // Only overwrite if update has a defined, non-null value
    if (value !== undefined && value !== null) {
      merged[key] = value;
    }
  }
  return merged;
};

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * Handle run metadata merging for chat messages
 *
 * Features:
 * - Smart merging of metadata fields
 * - Workspace state fallbacks
 * - Trace ID tracking
 */
export function useRunMetadataHandler(
  options: UseRunMetadataHandlerOptions
): UseRunMetadataHandlerReturn {
  const { setMessages, streamingMessageId, workspaceActiveState, onTraceIdUpdate } = options;

  const mergeRunMetadata = useCallback(
    (metadata: RunMetadata) => {
      setMessages((prev) =>
        prev.map((message) => {
          const key = message.traceId || message.requestId || message.id;
          const matches =
            (metadata.traceId && metadata.traceId === key) ||
            (metadata.requestId && metadata.requestId === key) ||
            (streamingMessageId && message.id === streamingMessageId);

          if (!matches) {
            return message;
          }

          const existing = message.runMetadata ?? {};
          const merged = mergeDefinedFields(
            existing as Record<string, unknown>,
            metadata as Record<string, unknown>
          );
          const resolvedSeedMaterial = metadata.seedMaterial ?? existing.seedMaterial;
          const nextRunMetadata: RunMetadata & Record<string, unknown> = {
            ...merged,
            requestId: metadata.requestId ?? existing.requestId ?? message.requestId,
            traceId: metadata.traceId ?? existing.traceId ?? message.traceId,
            planId: metadata.planId ?? existing.planId ?? workspaceActiveState?.activePlanId ?? undefined,
            manifestHashB3:
              metadata.manifestHashB3 ?? existing.manifestHashB3 ?? workspaceActiveState?.manifestHashB3 ?? undefined,
            policyMaskDigestB3:
              metadata.policyMaskDigestB3 ??
              existing.policyMaskDigestB3 ??
              workspaceActiveState?.policyMaskDigestB3 ??
              undefined,
            seededViaHkdf:
              metadata.seededViaHkdf ??
              existing.seededViaHkdf ??
              (resolvedSeedMaterial ? true : undefined),
            seedMaterial: resolvedSeedMaterial,
          };

          return { ...message, runMetadata: nextRunMetadata };
        })
      );
    },
    [
      setMessages,
      streamingMessageId,
      workspaceActiveState?.activePlanId,
      workspaceActiveState?.manifestHashB3,
      workspaceActiveState?.policyMaskDigestB3,
    ]
  );

  const handleRunMetadata = useCallback(
    (metadata: RunMetadata) => {
      if (metadata.traceId && onTraceIdUpdate) {
        onTraceIdUpdate(metadata.traceId);
      }
      mergeRunMetadata(metadata);
    },
    [mergeRunMetadata, onTraceIdUpdate]
  );

  return {
    mergeRunMetadata,
    handleRunMetadata,
  };
}
