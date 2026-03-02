import { request, type FullConfig } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

function sanitizeRunId(value: string): string {
  const cleaned = value.trim().replace(/[^A-Za-z0-9._-]/g, '_');
  return cleaned || 'default';
}

function parseServerPort(value: string | undefined): number {
  const parsed = Number.parseInt((value ?? '18080').trim(), 10);
  if (!Number.isFinite(parsed) || parsed < 1 || parsed > 65535) {
    return 18080;
  }
  return parsed;
}

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..', '..', '..');
const runId = sanitizeRunId(process.env.PW_RUN_ID ?? 'default');
const serverPort = parseServerPort(process.env.PW_SERVER_PORT);
const runRoot = path.resolve(repoRoot, 'var/playwright/runs', runId);
const backendBaseUrl = `http://localhost:${serverPort}`;
const storageStatePath = path.resolve(runRoot, 'storageState.json');
const debugDir = path.resolve(runRoot, 'debug');
const heartbeatPath = path.resolve(runRoot, 'heartbeat.json');
const serverPidPath = path.resolve(runRoot, 'run/aos-server.pid');
const singleWriterPidPath = path.resolve(runRoot, 'run/aos-cp-single-writer.pid');
const cleanDebugDir = (process.env.PW_CLEAN_DEBUG ?? '').trim() === '1';

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
  for (const filePath of [storageStatePath, heartbeatPath, serverPidPath, singleWriterPidPath]) {
    if (!fs.existsSync(filePath)) continue;
    try {
      fs.unlinkSync(filePath);
    } catch (err) {
      console.warn(`[playwright:teardown] Failed to remove ${filePath}: ${err}`);
    }
  }

  if (cleanDebugDir && fs.existsSync(debugDir)) {
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
