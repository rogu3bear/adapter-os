import { BackendName } from '@/api/types';

/**
 * Human-readable labels for backend names
 */
export const BACKEND_LABELS: Record<BackendName, string> = {
  auto: 'Auto (router)',
  coreml: 'CoreML',
  mlx: 'MLX',
  metal: 'Metal',
};

/**
 * Backend fallback priority order.
 * When a requested backend is unavailable, fall back through this list.
 */
export const BACKEND_PRIORITY: BackendName[] = ['coreml', 'mlx', 'metal', 'auto'];

/**
 * localStorage key for persisted backend preferences per model
 */
export const BACKEND_PREF_KEY = 'inference-backend-preferences';

/**
 * localStorage key for last selected model
 */
export const LAST_MODEL_KEY = 'inference-last-model';

/**
 * Maximum prompt length (characters)
 */
export const MAX_PROMPT_LENGTH = 32000;

/**
 * Adapter strength presets
 */
export const ADAPTER_STRENGTH_PRESETS = {
  light: 0.4,
  medium: 0.7,
  strong: 1.0,
} as const;

/**
 * Adapter lifecycle states that indicate readiness for inference
 */
export const READY_ADAPTER_STATES = ['hot', 'warm', 'resident'] as const;

/**
 * State indicator colors for adapter lifecycle states
 */
export const ADAPTER_STATE_COLORS: Record<string, { color: string; label: string }> = {
  resident: { color: 'bg-green-500', label: 'Resident' },
  hot: { color: 'bg-emerald-400', label: 'Hot' },
  warm: { color: 'bg-yellow-400', label: 'Warm' },
  cold: { color: 'bg-blue-400', label: 'Cold' },
  unloaded: { color: 'bg-gray-400', label: 'Unloaded' },
};
