import { lazy, type ComponentType } from 'react';
import { logger } from '@/utils/logger';

type Loader<T extends ComponentType> = () => Promise<{ default: T }>;

interface LazyWithRetryOptions {
  retries?: number;
  delayMs?: number;
  devReload?: boolean;
  timeoutMs?: number;
}

const wait = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

const isRecoverableChunkError = (err: unknown): err is Error => {
  if (!(err instanceof Error)) return false;
  const message = err.message || '';
  return (
    message.includes('ChunkLoadError') ||
    message.includes('Loading chunk') ||
    message.includes('Failed to fetch dynamically imported module') ||
    message.includes('Importing a module script failed')
  );
};

/**
 * Wrap React.lazy to tolerate transient dev chunk load failures.
 * Retries a few times, then optionally reloads (dev only) before surfacing the error.
 */
export function lazyWithRetry<T extends ComponentType>(
  factory: Loader<T>,
  options: LazyWithRetryOptions = {},
) {
  const { retries = 2, delayMs = 400, devReload = true, timeoutMs = 30000 } = options;
  let attempt = 0;

  const loadWithTimeout = (): Promise<{ default: T }> => {
    return Promise.race([
      factory(),
      new Promise<never>((_, reject) => {
        setTimeout(() => {
          reject(new Error(`Component load timeout after ${timeoutMs}ms`));
        }, timeoutMs);
      }),
    ]);
  };

  const load = (): Promise<{ default: T }> =>
    loadWithTimeout().catch(async (err) => {
      attempt += 1;
      if (isRecoverableChunkError(err) && attempt <= retries) {
        const backoff = delayMs * attempt;
        logger.warn(
          `[lazyWithRetry] retrying chunk load in ${backoff}ms (attempt ${attempt})`,
          { component: 'lazyWithRetry', details: String(err) },
          err instanceof Error ? err : new Error(String(err))
        );
        await wait(backoff);
        return load();
      }

      if (isRecoverableChunkError(err) && import.meta.env.DEV && devReload) {
        logger.warn(
          '[lazyWithRetry] reloading page after chunk load failure',
          { component: 'lazyWithRetry', details: String(err) },
          err instanceof Error ? err : new Error(String(err))
        );
        window.location.reload();
      }

      throw err;
    });

  return lazy(load);
}

