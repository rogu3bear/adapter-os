/**
 * Auth Hooks
 *
 * Barrel exports for authentication-related hooks.
 */

export { useAuthFlow } from './useAuthFlow';
export type {
  AuthFlowState,
  AuthFlowError,
  LoginCredentials,
  UseAuthFlowReturn,
} from './useAuthFlow';

export { useHealthPolling } from './useHealthPolling';
export type {
  BackendStatus,
  HealthState,
  UseHealthPollingReturn,
} from './useHealthPolling';
