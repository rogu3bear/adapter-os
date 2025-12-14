/**
 * usePolicies - Policy management hooks (compatibility wrapper)
 *
 * This is a compatibility wrapper that exports hooks from useSecurity.ts
 * to maintain backward compatibility with existing imports.
 */

export {
  usePolicies,
  usePolicyDetail,
  usePolicyMutations,
} from './useSecurity';
