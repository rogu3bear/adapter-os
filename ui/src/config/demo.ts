import type { SessionMode } from '@/api/auth-types';

const readEnv = (): Record<string, string | undefined> => {
  const meta = typeof import.meta !== 'undefined' ? (import.meta as { env?: Record<string, string> }) : undefined;
  return meta?.env ?? {};
};

export function isDemoEnvEnabled(): boolean {
  const env = readEnv();
  return env.VITE_DEMO_MODE === 'true';
}

export function isDemoSessionMode(sessionMode?: SessionMode | null): boolean {
  return sessionMode === 'dev_bypass';
}

export function isDemoMvpMode(sessionMode?: SessionMode | null): boolean {
  return isDemoEnvEnabled() || isDemoSessionMode(sessionMode);
}

export function getDemoEntryPath(): string {
  return '/chat';
}

