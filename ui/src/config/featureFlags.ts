/**
 * Centralized feature flag helpers.
 *
 * Flags are sourced from Vite environment variables (VITE_*). Defaults are
 * false unless explicitly set to the string "true".
 */

type EnvReader = {
  env?: Record<string, string | undefined>;
};

const env = (import.meta as unknown as EnvReader).env || {};

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
 */
export function isFeatureEnabled(flagName: string): boolean {
  const key = `VITE_${flagName}`;
  return env[key] === 'true';
}
