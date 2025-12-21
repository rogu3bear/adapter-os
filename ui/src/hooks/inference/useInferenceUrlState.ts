import { useCallback, useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';
import { InferenceUrlParams } from '@/components/inference/types';

export interface UseInferenceUrlStateReturn {
  /** Initial state parsed from URL */
  initialState: InferenceUrlParams;
  /** Update a single URL parameter */
  updateUrl: (key: keyof InferenceUrlParams, value: string | undefined) => void;
  /** Update multiple URL parameters at once */
  updateUrlBatch: (params: Partial<InferenceUrlParams>) => void;
  /** Clear all inference URL state */
  clearUrlState: () => void;
  /** Get current URL state */
  getCurrentState: () => InferenceUrlParams;
}

const URL_PARAM_KEYS: (keyof InferenceUrlParams)[] = [
  'modelId',
  'adapterId',
  'stackId',
  'backend',
];

// Map between our state keys and URL param names
const URL_PARAM_MAP: Record<keyof InferenceUrlParams, string> = {
  modelId: 'model',
  adapterId: 'adapter',
  stackId: 'stack',
  backend: 'backend',
};

/**
 * Hook for synchronizing inference state with URL parameters.
 * Enables shareable/bookmarkable inference configurations.
 */
export function useInferenceUrlState(): UseInferenceUrlStateReturn {
  const [searchParams, setSearchParams] = useSearchParams();

  // Parse initial state from URL
  const initialState = useMemo((): InferenceUrlParams => {
    return {
      modelId: searchParams.get(URL_PARAM_MAP.modelId) || undefined,
      adapterId: searchParams.get(URL_PARAM_MAP.adapterId) || undefined,
      stackId: searchParams.get(URL_PARAM_MAP.stackId) || undefined,
      backend: searchParams.get(URL_PARAM_MAP.backend) || undefined,
    };
  }, [searchParams]);

  // Update a single URL parameter
  const updateUrl = useCallback(
    (key: keyof InferenceUrlParams, value: string | undefined) => {
      const urlKey = URL_PARAM_MAP[key];
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          if (value) {
            next.set(urlKey, value);
          } else {
            next.delete(urlKey);
          }
          return next;
        },
        { replace: true }
      );
    },
    [setSearchParams]
  );

  // Update multiple URL parameters at once
  const updateUrlBatch = useCallback(
    (params: Partial<InferenceUrlParams>) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          for (const [key, value] of Object.entries(params)) {
            const urlKey = URL_PARAM_MAP[key as keyof InferenceUrlParams];
            if (value) {
              next.set(urlKey, value);
            } else {
              next.delete(urlKey);
            }
          }
          return next;
        },
        { replace: true }
      );
    },
    [setSearchParams]
  );

  // Clear all inference URL state
  const clearUrlState = useCallback(() => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        for (const key of URL_PARAM_KEYS) {
          next.delete(URL_PARAM_MAP[key]);
        }
        return next;
      },
      { replace: true }
    );
  }, [setSearchParams]);

  // Get current URL state
  const getCurrentState = useCallback((): InferenceUrlParams => {
    return {
      modelId: searchParams.get(URL_PARAM_MAP.modelId) || undefined,
      adapterId: searchParams.get(URL_PARAM_MAP.adapterId) || undefined,
      stackId: searchParams.get(URL_PARAM_MAP.stackId) || undefined,
      backend: searchParams.get(URL_PARAM_MAP.backend) || undefined,
    };
  }, [searchParams]);

  return {
    initialState,
    updateUrl,
    updateUrlBatch,
    clearUrlState,
    getCurrentState,
  };
}
