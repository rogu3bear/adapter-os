/**
 * Feature flag hooks for runtime configuration
 *
 * Provides hooks for accessing feature flags via environment variables.
 * All feature flags default to `false` unless explicitly enabled.
 */

/**
 * Hook to check if chat auto-load models feature is enabled.
 *
 * When enabled, the chat interface will automatically attempt to load
 * required base models when starting a new chat session, rather than
 * requiring manual model loading by the user.
 *
 * @returns `true` if auto-load is enabled, `false` otherwise
 *
 * @example
 * ```tsx
 * const ChatPage = () => {
 *   const autoLoadEnabled = useChatAutoLoadModels();
 *
 *   useEffect(() => {
 *     if (autoLoadEnabled && !modelLoaded) {
 *       loadBaseModel();
 *     }
 *   }, [autoLoadEnabled, modelLoaded]);
 * };
 * ```
 *
 * Configuration:
 * - Set `VITE_CHAT_AUTO_LOAD_MODELS=true` in `.env` file
 * - Defaults to `false` if not set or set to any other value
 */
export function useChatAutoLoadModels(): boolean {
  return import.meta.env.VITE_CHAT_AUTO_LOAD_MODELS === 'true';
}

/**
 * General-purpose feature flag checker.
 *
 * @param flagName - The environment variable name (without VITE_ prefix)
 * @returns `true` if the flag is set to 'true', `false` otherwise
 *
 * @example
 * ```tsx
 * const debugMode = useFeatureFlag('DEBUG_MODE');
 * // Checks import.meta.env.VITE_DEBUG_MODE
 * ```
 */
export function useFeatureFlag(flagName: string): boolean {
  const envKey = `VITE_${flagName}` as keyof ImportMetaEnv;
  return import.meta.env[envKey] === 'true';
}
