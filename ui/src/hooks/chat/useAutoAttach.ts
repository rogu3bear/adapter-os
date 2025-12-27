import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useChatContext, type SuggestedAdapter } from '@/contexts/ChatContext';
import { logger, toError } from '@/utils/logger';
import { useTopologyData } from '../topology/useTopologyData';
import { resolveAdapterConflicts, type ConflictCheck } from '@/utils/adapterMagnet';

interface UseAutoAttachOptions {
  text: string;
  autoAttachEnabled: boolean;
  confidenceThreshold?: number;
  debounceMs?: number;
  stackId?: string | null;
  tenantId?: string;
}

const DEFAULT_CONFIDENCE_THRESHOLD = 0.85;
const DEFAULT_DEBOUNCE_MS = 500;

export function useAutoAttach(options: UseAutoAttachOptions) {
  const {
    text,
    autoAttachEnabled,
    confidenceThreshold = DEFAULT_CONFIDENCE_THRESHOLD,
    debounceMs = DEFAULT_DEBOUNCE_MS,
  } = options;

  const {
    suggestedAdapters,
    attachedAdapters,
    setSuggestedAdapters,
    attachAdapter,
    removeAttachedAdapter,
    lastAttachedAdapterId,
    autoAttachPaused,
    resumeAutoAttach,
    pauseAutoAttach,
    mutedAdapterIds,
    muteAdapter,
  } = useChatContext();

  const normalizedText = text.trim();
  const [suggestions, setSuggestions] = useState<SuggestedAdapter[]>([]);
  const [predictionError, setPredictionError] = useState<string | null>(null);
  const mutedSize = mutedAdapterIds?.size ?? 0;

  const topologyResult = useTopologyData({
    enabled: !autoAttachPaused && autoAttachEnabled && Boolean(normalizedText),
    previewText: normalizedText,
    debounceMs,
  });

  const predictedSuggestions = useMemo(() => {
    const path = topologyResult.data?.predictedPath ?? [];
    const mapped: SuggestedAdapter[] = [];

    path.forEach((node, idx) => {
      const adapterId = node.adapterId ?? node.id;
      if (!adapterId) return;
      const rawConfidence = typeof node.confidence === 'number' && Number.isFinite(node.confidence)
        ? node.confidence
        : Math.max(0, 1 - idx * 0.1);

      mapped.push({
        id: adapterId,
        confidence: rawConfidence,
        reason: node.clusterId ? `Cluster ${node.clusterId}` : node.kind,
        source: 'router',
      });
    });

    return mapped.sort((a, b) => (b.confidence - a.confidence) || a.id.localeCompare(b.id));
  }, [topologyResult.data?.predictedPath]);

  useEffect(() => {
    if (!autoAttachEnabled || !normalizedText) {
      setSuggestions([]);
      setPredictionError(null);
      return;
    }

    if (topologyResult.error) {
      const normalizedError = toError(topologyResult.error);
      setPredictionError(normalizedError.message);
      logger.error('Failed to project topology path', {
        component: 'useAutoAttach',
      }, normalizedError);

      setSuggestions([]);
      return;
    }

    setPredictionError(null);
    const filtered = predictedSuggestions.filter((item) => !mutedAdapterIds?.has(item.id));
    setSuggestions(filtered);
  }, [
    autoAttachEnabled,
    mutedAdapterIds,
    normalizedText,
    predictedSuggestions,
    mutedSize,
    topologyResult.error,
  ]);

  const lastAutoAttachRef = useRef<string | null>(null);
  const [conflictState, setConflictState] = useState<{
    candidateId: string;
    conflicts: string[];
    reason?: string;
    resolution?: ConflictCheck['decision'];
  } | null>(null);

  const attachWithResolution = useCallback(
    (
      adapter: SuggestedAdapter,
      attachedBy: 'auto' | 'manual' = 'manual',
      options: { forceReplace?: boolean } = {}
    ) => {
      const resolution = resolveAdapterConflicts(adapter, attachedAdapters);

      if (resolution.decision === 'skip') {
        setConflictState(null);
        return { attached: false, resolution };
      }

      if (resolution.conflicts.length > 0 && !options.forceReplace) {
        setConflictState({
          candidateId: adapter.id,
          conflicts: resolution.conflicts.map((c) => c.id),
          reason: resolution.reason,
          resolution: resolution.decision,
        });
        pauseAutoAttach();
        return { attached: false, resolution };
      }

      if (resolution.conflicts.length > 0 && options.forceReplace) {
        resolution.conflicts.forEach((conflict) => removeAttachedAdapter(conflict.id));
      }

      attachAdapter(adapter, attachedBy);
      setConflictState(null);
      resumeAutoAttach();

      return { attached: true, resolution };
    },
    [attachAdapter, attachedAdapters, pauseAutoAttach, removeAttachedAdapter, resumeAutoAttach]
  );

  useEffect(() => {
    setSuggestedAdapters(suggestions);
    if (suggestions.length > 0) {
      resumeAutoAttach();
    }
  }, [resumeAutoAttach, setSuggestedAdapters, suggestions]);

  useEffect(() => {
    if (conflictState && suggestions.every((s) => s.id !== conflictState.candidateId)) {
      setConflictState(null);
      resumeAutoAttach();
    }
  }, [conflictState, resumeAutoAttach, suggestions]);

  useEffect(() => {
    if (!autoAttachEnabled || autoAttachPaused) {
      lastAutoAttachRef.current = null;
      setConflictState(null);
      return;
    }

    const candidate = suggestions.find((s) => s.confidence >= confidenceThreshold);
    if (!candidate) return;

    const alreadyAttached = attachedAdapters.some((adapter) => adapter.id === candidate.id);
    if (alreadyAttached) return;

    if (lastAutoAttachRef.current === candidate.id) return;

    const result = attachWithResolution(candidate, 'auto');
    if (!result.attached && result.resolution?.conflicts?.length) {
      logger.debug('Auto-attach blocked by conflict', {
        component: 'useAutoAttach',
        adapterId: candidate.id,
        conflicts: result.resolution.conflicts.map((c) => c.id),
      });
    } else if (result.attached) {
      lastAutoAttachRef.current = candidate.id;
      logger.debug('Auto-attached adapter', {
        component: 'useAutoAttach',
        adapterId: candidate.id,
        confidence: candidate.confidence,
      });
    }
  }, [
    attachAdapter,
    attachedAdapters,
    attachWithResolution,
    autoAttachEnabled,
    autoAttachPaused,
    confidenceThreshold,
    suggestions,
  ]);

  const autoAttachPausedOrLagging = useMemo(() => autoAttachPaused, [autoAttachPaused]);
  const bestSuggestion = useMemo(() => suggestions[0] ?? null, [suggestions]);

  return {
    suggestedAdapters,
    attachedAdapters,
    lastAttachedAdapterId,
    autoAttachPaused: autoAttachPausedOrLagging,
    attachAdapter,
    attachWithResolution,
    removeAttachedAdapter,
    predictionLoading: topologyResult.isFetching,
    predictionError,
    bestSuggestion,
    muteAdapter,
    mutedAdapterIds,
    conflictState,
  };
}

export default useAutoAttach;
