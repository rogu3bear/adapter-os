/**
 * Centralized feature flag helpers.
 *
 * Flags are sourced from Vite environment variables (VITE_*). Defaults are
 * defined in FLAG_DEFAULTS below; if not present, defaults to false.
 */

type EnvReader = {
  env?: Record<string, string | undefined>;
};

const env = (import.meta as unknown as EnvReader).env || {};

/**
 * Default values for feature flags when not set via environment variables.
 * Keys should match the flag name without the VITE_ prefix.
 */
const FLAG_DEFAULTS: Record<string, boolean> = {
  COREML_EXPORT_UI: false,
  CHAT_AUTO_LOAD_MODELS: true,
};

/**
 * Returns true when CoreML export/verification UI should be shown.
 *
 * Configure via VITE_COREML_EXPORT_UI=true
 */
export function isCoremlPackageUiEnabled(): boolean {
  return env.VITE_COREML_EXPORT_UI === 'true';
}

/**
 * Generic helper for ad-hoc flags.
 * Checks environment variable first, then falls back to FLAG_DEFAULTS.
 */
export function isFeatureEnabled(flagName: string): boolean {
  const key = `VITE_${flagName}`;
  const envValue = env[key];
  if (envValue !== undefined) {
    return envValue === 'true';
  }
  return FLAG_DEFAULTS[flagName] ?? false;
}
