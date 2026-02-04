import { request, type FullConfig } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const backendBaseUrl = 'http://localhost:8080';
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..', '..', '..');
const storageStatePath = path.resolve(repoRoot, 'var/playwright/storageState.json');
const debugDir = path.resolve(repoRoot, 'var/playwright/debug');

/** Best-effort API reset with timeout. */
async function resetBackend(): Promise<void> {
  const api = await request.newContext({ baseURL: backendBaseUrl });
  try {
    const resp = await api.post('/testkit/reset', { timeout: 10_000 });
    if (!resp.ok()) {
      console.warn(`[playwright:teardown] /testkit/reset returned ${resp.status()}`);
    }
  } finally {
    await api.dispose();
  }
}

/** Best-effort cleanup of filesystem artifacts. */
function cleanupFilesystem(): void {
  // Remove storage state file
  if (fs.existsSync(storageStatePath)) {
    try {
      fs.unlinkSync(storageStatePath);
    } catch (err) {
      console.warn(`[playwright:teardown] Failed to remove ${storageStatePath}: ${err}`);
    }
  }

  // Remove debug screenshots directory
  if (fs.existsSync(debugDir)) {
    try {
      fs.rmSync(debugDir, { recursive: true, force: true });
    } catch (err) {
      console.warn(`[playwright:teardown] Failed to remove ${debugDir}: ${err}`);
    }
  }
}

export default async function globalTeardown(_config: FullConfig) {
  // Reset backend state
  try {
    await resetBackend();
  } catch (err) {
    console.warn(`[playwright:teardown] API reset failed: ${err}`);
  }

  // Clean up filesystem artifacts
  try {
    cleanupFilesystem();
  } catch (err) {
    console.warn(`[playwright:teardown] Filesystem cleanup failed: ${err}`);
  }
}
