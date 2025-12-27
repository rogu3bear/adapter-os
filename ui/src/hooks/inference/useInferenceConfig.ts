// @ts-nocheck
import { useState, useCallback } from 'react';
import type { InferenceConfig, InferResponse } from '@/api/types';

/**
 * Hook for managing dual inference configurations (A/B comparison mode)
 *
 * @example
 * ```typescript
 * const {
 *   configA,
 *   configB,
 *   setConfigA,
 *   setConfigB,
 *   responseA,
 *   responseB,
 *   isLoadingA,
 *   isLoadingB,
 *   resetConfig,
 *   resetAll
 * } = useInferenceConfig({
 *   defaultMaxTokens: 100,
 *   defaultTemperature: 0.7
 * });
 *
 * // Update config A
 * setConfigA({ ...configA, temperature: 0.8 });
 *
 * // Reset config B to defaults
 * resetConfig('b');
 *
 * // Reset both configs
 * resetAll();
 * ```
 */

export interface UseInferenceConfigOptions {
  defaultMaxTokens?: number;
  defaultTemperature?: number;
}

export interface UseInferenceConfigReturn {
  configA: InferenceConfig;
  configB: InferenceConfig;
  setConfigA: (config: InferenceConfig | ((prev: InferenceConfig) => InferenceConfig)) => void;
  setConfigB: (config: InferenceConfig | ((prev: InferenceConfig) => InferenceConfig)) => void;
  responseA: InferResponse | null;
  responseB: InferResponse | null;
  setResponseA: (response: InferResponse | null) => void;
  setResponseB: (response: InferResponse | null) => void;
  isLoadingA: boolean;
  isLoadingB: boolean;
  setIsLoadingA: (loading: boolean) => void;
  setIsLoadingB: (loading: boolean) => void;
  resetConfig: (id: 'a' | 'b') => void;
  resetAll: () => void;
}

const DEFAULT_MAX_TOKENS = 100;
const DEFAULT_TEMPERATURE_A = 0.7;
const DEFAULT_TEMPERATURE_B = 0.9;
const DEFAULT_TOP_K = 50;
const DEFAULT_TOP_P = 0.9;

export function useInferenceConfig(
  options: UseInferenceConfigOptions = {}
): UseInferenceConfigReturn {
  const {
    defaultMaxTokens = DEFAULT_MAX_TOKENS,
    defaultTemperature = DEFAULT_TEMPERATURE_A,
  } = options;

  // Create default config generator
  const createDefaultConfig = useCallback(
    (id: 'a' | 'b'): InferenceConfig => ({
      id,
      prompt: '',
      max_tokens: defaultMaxTokens,
      temperature: id === 'a' ? defaultTemperature : DEFAULT_TEMPERATURE_B,
      top_k: DEFAULT_TOP_K,
      top_p: DEFAULT_TOP_P,
      backend: 'auto',
      seed: undefined,
      require_evidence: false,
      routing_determinism_mode: 'deterministic',
    }),
    [defaultMaxTokens, defaultTemperature]
  );

  // Inference configurations
  const [configA, setConfigA] = useState<InferenceConfig>(() => createDefaultConfig('a'));
  const [configB, setConfigB] = useState<InferenceConfig>(() => createDefaultConfig('b'));

  // Responses
  const [responseA, setResponseA] = useState<InferResponse | null>(null);
  const [responseB, setResponseB] = useState<InferResponse | null>(null);

  // Loading states
  const [isLoadingA, setIsLoadingA] = useState(false);
  const [isLoadingB, setIsLoadingB] = useState(false);

  // Reset individual config to defaults
  const resetConfig = useCallback(
    (id: 'a' | 'b') => {
      const defaultConfig = createDefaultConfig(id);
      if (id === 'a') {
        setConfigA(defaultConfig);
        setResponseA(null);
        setIsLoadingA(false);
      } else {
        setConfigB(defaultConfig);
        setResponseB(null);
        setIsLoadingB(false);
      }
    },
    [createDefaultConfig]
  );

  // Reset all configs and responses
  const resetAll = useCallback(() => {
    setConfigA(createDefaultConfig('a'));
    setConfigB(createDefaultConfig('b'));
    setResponseA(null);
    setResponseB(null);
    setIsLoadingA(false);
    setIsLoadingB(false);
  }, [createDefaultConfig]);

  return {
    configA,
    configB,
    setConfigA,
    setConfigB,
    responseA,
    responseB,
    setResponseA,
    setResponseB,
    isLoadingA,
    isLoadingB,
    setIsLoadingA,
    setIsLoadingB,
    resetConfig,
    resetAll,
  };
}
