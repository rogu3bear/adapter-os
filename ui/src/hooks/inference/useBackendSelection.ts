import { useState, useEffect, useCallback } from 'react';
import { apiClient } from '@/api/services';
import { BackendName, BackendStatus, BackendCapability, HardwareCapabilities } from '@/api/types';
import { BackendOption, BackendSelectionResult } from '@/components/inference/types';
import { BACKEND_LABELS, BACKEND_PRIORITY, BACKEND_PREF_KEY } from '@/components/inference/constants';
import { toast } from 'sonner';

export interface UseBackendSelectionOptions {
  /** Model ID to use for preference lookup */
  modelId?: string;
  /** Initial backend preference */
  initialBackend?: BackendName;
}

export interface UseBackendSelectionReturn {
  /** Available backend options with status */
  backendOptions: BackendOption[];
  /** Loading state for backend capabilities */
  isLoading: boolean;
  /** Error message if backend fetch failed */
  error: string | null;
  /** Warning message (e.g., fallback notification) */
  warning: string | null;
  /** Hardware capabilities (ANE, GPU, etc.) */
  hardwareCapabilities: HardwareCapabilities | null;
  /** Currently selected backend */
  selectedBackend: BackendName;
  /** Last backend actually used in inference */
  lastBackendUsed: string | null;
  /** Select a backend (with fallback resolution) */
  selectBackend: (backend: BackendName) => BackendSelectionResult;
  /** Resolve backend for a request (applies fallback if needed) */
  resolveBackendForRequest: (requested?: BackendName) => BackendSelectionResult;
  /** Clear warning message */
  clearWarning: () => void;
  /** Set selected backend directly */
  setSelectedBackend: (backend: BackendName) => void;
  /** Set last backend used */
  setLastBackendUsed: (backend: string | null) => void;
}

/**
 * Hook for managing backend selection with automatic fallback.
 * Handles backend availability detection, preference persistence,
 * and graceful fallback when requested backends are unavailable.
 */
export function useBackendSelection(
  options: UseBackendSelectionOptions = {}
): UseBackendSelectionReturn {
  const { modelId = '', initialBackend = 'auto' } = options;

  // Backend preferences persisted to localStorage
  const [backendPreferences, setBackendPreferences] = useState<Record<string, BackendName>>(() => {
    try {
      const raw = localStorage.getItem(BACKEND_PREF_KEY);
      return raw ? (JSON.parse(raw) as Record<string, BackendName>) : {};
    } catch {
      return {};
    }
  });

  const [backendOptions, setBackendOptions] = useState<BackendOption[]>([
    { name: 'auto', available: true },
  ]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [warning, setWarning] = useState<string | null>(null);
  const [lastBackendUsed, setLastBackendUsed] = useState<string | null>(null);
  const [hardwareCapabilities, setHardwareCapabilities] = useState<HardwareCapabilities | null>(
    null
  );
  const [selectedBackend, setSelectedBackend] = useState<BackendName>(initialBackend);

  // Determine the default backend based on priority and availability
  const determineDefaultBackend = useCallback((): BackendName => {
    for (const backend of BACKEND_PRIORITY) {
      if (backend === 'auto') {
        return 'auto';
      }
      const option = backendOptions.find((o) => o.name === backend);
      if (option?.available) {
        return backend;
      }
    }
    return 'auto';
  }, [backendOptions]);

  // Persist backend preference to localStorage
  const persistBackendPreference = useCallback(
    (targetModelId: string, backend: BackendName) => {
      setBackendPreferences((prev) => {
        const next = { ...prev, [targetModelId || '__default']: backend };
        try {
          localStorage.setItem(BACKEND_PREF_KEY, JSON.stringify(next));
        } catch {
          // Best-effort persistence
        }
        return next;
      });
    },
    []
  );

  // Get preferred backend for a model
  const getPreferredBackend = useCallback(
    (targetModelId: string): BackendName => {
      const key = targetModelId || '__default';
      const stored = backendPreferences[key] || backendPreferences['__default'];
      return stored || determineDefaultBackend();
    },
    [backendPreferences, determineDefaultBackend]
  );

  // Resolve backend selection with fallback
  const resolveBackendForRequest = useCallback(
    (requested?: BackendName): BackendSelectionResult => {
      const target = requested || determineDefaultBackend();

      if (target === 'auto') {
        setWarning(null);
        return { backend: 'auto', reason: null };
      }

      const option = backendOptions.find((o) => o.name === target);
      if (!option) {
        const reason = 'Backend availability is unknown; using Auto.';
        setWarning(reason);
        return { backend: 'auto', reason };
      }

      if (option.available) {
        setWarning(null);
        return { backend: target, reason: null };
      }

      // Find fallback
      const startIndex = BACKEND_PRIORITY.indexOf(target);
      const fallbackChain =
        startIndex >= 0 ? BACKEND_PRIORITY.slice(startIndex + 1) : BACKEND_PRIORITY;

      const failedDetail = option.notes?.[0] || option.status || 'unavailable';

      for (const fallback of fallbackChain) {
        if (fallback === 'auto') {
          const reason = `${BACKEND_LABELS[target] || target} is unavailable; falling back to Auto.`;
          setWarning(reason);
          return { backend: 'auto', reason };
        }

        const fallbackOption = backendOptions.find((o) => o.name === fallback);
        if (fallbackOption?.available) {
          const reason = `Fell back from ${BACKEND_LABELS[target] || target} to ${BACKEND_LABELS[fallback] || fallback} (reason: ${failedDetail})`;
          setWarning(reason);
          return { backend: fallback, reason };
        }
      }

      const reason = `${BACKEND_LABELS[target] || target} is unavailable; falling back to Auto.`;
      setWarning(reason);
      return { backend: 'auto', reason };
    },
    [backendOptions, determineDefaultBackend]
  );

  // Select a backend (user action)
  const selectBackend = useCallback(
    (backend: BackendName): BackendSelectionResult => {
      const { backend: resolvedBackend, reason } = resolveBackendForRequest(backend);
      if (reason) {
        toast.info(reason);
      }
      setSelectedBackend(resolvedBackend);
      setLastBackendUsed(resolvedBackend);
      persistBackendPreference(modelId || '__default', resolvedBackend);
      return { backend: resolvedBackend, reason };
    },
    [modelId, persistBackendPreference, resolveBackendForRequest]
  );

  const clearWarning = useCallback(() => {
    setWarning(null);
  }, []);

  // Load backend availability/capabilities on mount
  useEffect(() => {
    let cancelled = false;

    const fetchBackends = async () => {
      setIsLoading(true);
      try {
        const [statusList, capabilities] = await Promise.all([
          apiClient.listBackends().catch(() => null),
          apiClient.getBackendCapabilities().catch(() => null),
        ]);

        if (cancelled) return;

        const statusByName = new Map(statusList?.backends?.map((b) => [b.backend, b]));
        const capabilityByName = new Map(
          capabilities?.backends?.map((b) => [b.backend, b.capabilities])
        );

        if (capabilities?.hardware) {
          setHardwareCapabilities(capabilities.hardware);
        }

        const options: BackendOption[] = (['auto', 'coreml', 'mlx', 'metal'] as BackendName[]).map(
          (name) => {
            if (name === 'auto') {
              return { name, available: true, status: 'healthy' };
            }
            const status = statusByName.get(name) as BackendStatus | undefined;
            const capability = capabilityByName.get(name) as BackendCapability[] | undefined;
            const isAvailable = Boolean(
              capability?.some((c) => c.available) && status?.status !== 'unavailable'
            );
            const hardwareHint =
              name === 'coreml' && capabilities?.hardware?.ane_available
                ? 'ANE + GPU'
                : name === 'metal' && capabilities?.hardware?.gpu_available
                  ? capabilities.hardware?.gpu_type || 'GPU'
                  : undefined;

            return {
              name,
              available: isAvailable,
              status: status?.status,
              mode: status?.mode,
              notes: status?.warnings || status?.notes,
              hardwareHint,
            };
          }
        );

        setBackendOptions(options);
        setError(null);
      } catch (err) {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : 'Failed to load backend capabilities');
        setBackendOptions([{ name: 'auto', available: true }]);
      } finally {
        if (!cancelled) {
          setIsLoading(false);
        }
      }
    };

    fetchBackends();
    return () => {
      cancelled = true;
    };
  }, []);

  // Update selected backend when model changes (use stored preference)
  useEffect(() => {
    if (!modelId || !backendOptions.length) return;

    const stored = backendPreferences[modelId] || backendPreferences['__default'];
    if (stored) {
      const { backend } = resolveBackendForRequest(stored);
      setSelectedBackend(backend);
    } else {
      const preferred = determineDefaultBackend();
      setSelectedBackend(preferred);

      if (preferred !== 'coreml') {
        const coremlOption = backendOptions.find((o) => o.name === 'coreml');
        const detail = coremlOption?.notes?.[0] || coremlOption?.status || 'CoreML unavailable';
        setWarning(
          `Fell back from CoreML to ${BACKEND_LABELS[preferred] || preferred} (reason: ${detail})`
        );
      }
    }
  }, [
    backendOptions,
    backendPreferences,
    determineDefaultBackend,
    modelId,
    resolveBackendForRequest,
  ]);

  return {
    backendOptions,
    isLoading,
    error,
    warning,
    hardwareCapabilities,
    selectedBackend,
    lastBackendUsed,
    selectBackend,
    resolveBackendForRequest,
    clearWarning,
    setSelectedBackend,
    setLastBackendUsed,
  };
}
