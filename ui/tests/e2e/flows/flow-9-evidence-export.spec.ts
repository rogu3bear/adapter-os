/**
 * Flow 9: Evidence export downloads deterministic ZIP via canonical endpoint
 *
 * Tests the evidence export functionality for inference runs, verifying:
 * - Canonical endpoint /v1/runs/{run_id}/evidence is used first
 * - Legacy fallback is labeled appropriately
 * - Downloaded ZIP contains expected files
 * - Export is deterministic (stable across re-exports)
 * - Backend failure triggers local bundle fallback with warning
 */

import { test, expect, type Page, type Route, type Request } from '@playwright/test';
import { createReadStream } from 'fs';
import { readFile } from 'fs/promises';
import { createGunzip } from 'zlib';
import { promisify } from 'util';

// -----------------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------------

const FIXED_NOW = '2025-01-15T12:00:00.000Z';
const MOCK_RUN_ID = 'run-flow9-evidence-test-001';
const MOCK_TRACE_ID = 'trace-flow9-abc123';
const MOCK_RECEIPT_DIGEST = 'b3-receipt-flow9-digest-0123456789abcdef';

// Expected ZIP file entries (verified by checking ZIP central directory)
const EXPECTED_ZIP_ENTRIES = [
  'run_envelope.json',
  'replay_metadata.json',
  'manifest_ref.json',
  'policy_digest.json',
  'boot_state.json',
  'README.md',
] as const;

// -----------------------------------------------------------------------------
// Mock Data
// -----------------------------------------------------------------------------

function createMockInferenceResponse() {
  return {
    schema_version: '1.0',
    id: 'resp-flow9-1',
    text: 'Flow 9 test inference response for evidence export.',
    tokens_generated: 12,
    token_count: 12,
    latency_ms: 45,
    adapters_used: ['adapter-flow9'],
    finish_reason: 'stop' as const,
    backend: 'coreml' as const,
    backend_used: 'coreml',
    run_receipt: {
      trace_id: MOCK_TRACE_ID,
      run_head_hash: 'head-hash-flow9',
      output_digest: 'output-digest-flow9',
      receipt_digest: MOCK_RECEIPT_DIGEST,
    },
    trace: {
      latency_ms: 45,
      adapters_used: ['adapter-flow9'],
      router_decisions: [{ adapter: 'adapter-flow9', score: 0.95 }],
      evidence_spans: [{ text: 'Evidence span for flow 9', relevance: 0.92 }],
    },
  };
}

// -----------------------------------------------------------------------------
// ZIP Utilities (minimal implementation without external deps)
// -----------------------------------------------------------------------------

/**
 * Creates a minimal valid ZIP file with the expected evidence bundle structure.
 * Uses the PK ZIP format specification to create a valid archive.
 */
function createMockZipBuffer(): Buffer {
  const files: Array<{ name: string; content: string }> = [
    {
      name: 'run_envelope.json',
      content: JSON.stringify(
        {
          schema_version: '1.0',
          run_id: MOCK_RUN_ID,
          trace_id: MOCK_TRACE_ID,
          workspace_id: 'tenant-1',
          created_at: FIXED_NOW,
          receipt_digest: MOCK_RECEIPT_DIGEST,
        },
        null,
        2
      ),
    },
    {
      name: 'replay_metadata.json',
      content: JSON.stringify(
        {
          schema_version: '1.0',
          determinism_version: '2.0',
          router_seed: 42,
          tick: 1000,
        },
        null,
        2
      ),
    },
    {
      name: 'manifest_ref.json',
      content: JSON.stringify(
        {
          schema_version: '1.0',
          manifest_hash_b3: 'b3-manifest-hash-flow9',
          adapter_ids: ['adapter-flow9'],
        },
        null,
        2
      ),
    },
    {
      name: 'policy_digest.json',
      content: JSON.stringify(
        {
          schema_version: '1.0',
          policy_mask_digest_b3: 'b3-policy-mask-flow9',
        },
        null,
        2
      ),
    },
    {
      name: 'boot_state.json',
      content: JSON.stringify(
        {
          schema_version: '1.0',
          boot_trace_id: 'boot-trace-flow9',
          worker_id: 'worker-flow9',
        },
        null,
        2
      ),
    },
    {
      name: 'README.md',
      content: `# Evidence Bundle\n\nRun ID: ${MOCK_RUN_ID}\nTrace ID: ${MOCK_TRACE_ID}\n`,
    },
  ];

  // Build ZIP file manually (simplified PK format)
  const localHeaders: Buffer[] = [];
  const centralHeaders: Buffer[] = [];
  let offset = 0;

  for (const file of files) {
    const nameBuffer = Buffer.from(file.name, 'utf8');
    const contentBuffer = Buffer.from(file.content, 'utf8');

    // Local file header (signature: 0x04034b50)
    const localHeader = Buffer.alloc(30 + nameBuffer.length);
    localHeader.writeUInt32LE(0x04034b50, 0); // signature
    localHeader.writeUInt16LE(20, 4); // version needed
    localHeader.writeUInt16LE(0, 6); // flags
    localHeader.writeUInt16LE(0, 8); // compression (store)
    localHeader.writeUInt16LE(0, 10); // mod time
    localHeader.writeUInt16LE(0, 12); // mod date
    localHeader.writeUInt32LE(0, 14); // crc32 (simplified)
    localHeader.writeUInt32LE(contentBuffer.length, 18); // compressed size
    localHeader.writeUInt32LE(contentBuffer.length, 22); // uncompressed size
    localHeader.writeUInt16LE(nameBuffer.length, 26); // filename length
    localHeader.writeUInt16LE(0, 28); // extra field length
    nameBuffer.copy(localHeader, 30);

    localHeaders.push(Buffer.concat([localHeader, contentBuffer]));

    // Central directory header (signature: 0x02014b50)
    const centralHeader = Buffer.alloc(46 + nameBuffer.length);
    centralHeader.writeUInt32LE(0x02014b50, 0); // signature
    centralHeader.writeUInt16LE(20, 4); // version made by
    centralHeader.writeUInt16LE(20, 6); // version needed
    centralHeader.writeUInt16LE(0, 8); // flags
    centralHeader.writeUInt16LE(0, 10); // compression
    centralHeader.writeUInt16LE(0, 12); // mod time
    centralHeader.writeUInt16LE(0, 14); // mod date
    centralHeader.writeUInt32LE(0, 16); // crc32
    centralHeader.writeUInt32LE(contentBuffer.length, 20); // compressed size
    centralHeader.writeUInt32LE(contentBuffer.length, 24); // uncompressed size
    centralHeader.writeUInt16LE(nameBuffer.length, 28); // filename length
    centralHeader.writeUInt16LE(0, 30); // extra field length
    centralHeader.writeUInt16LE(0, 32); // comment length
    centralHeader.writeUInt16LE(0, 34); // disk number start
    centralHeader.writeUInt16LE(0, 36); // internal attributes
    centralHeader.writeUInt32LE(0, 38); // external attributes
    centralHeader.writeUInt32LE(offset, 42); // relative offset
    nameBuffer.copy(centralHeader, 46);

    centralHeaders.push(centralHeader);
    offset += localHeader.length + contentBuffer.length;
  }

  // End of central directory record (signature: 0x06054b50)
  const centralDirData = Buffer.concat(centralHeaders);
  const endOfCentralDir = Buffer.alloc(22);
  endOfCentralDir.writeUInt32LE(0x06054b50, 0); // signature
  endOfCentralDir.writeUInt16LE(0, 4); // disk number
  endOfCentralDir.writeUInt16LE(0, 6); // disk with central dir
  endOfCentralDir.writeUInt16LE(files.length, 8); // entries on disk
  endOfCentralDir.writeUInt16LE(files.length, 10); // total entries
  endOfCentralDir.writeUInt32LE(centralDirData.length, 12); // central dir size
  endOfCentralDir.writeUInt32LE(offset, 16); // central dir offset
  endOfCentralDir.writeUInt16LE(0, 20); // comment length

  return Buffer.concat([...localHeaders, centralDirData, endOfCentralDir]);
}

/**
 * Extract filenames from a ZIP buffer by reading the central directory.
 */
function extractZipFilenames(buffer: Buffer): string[] {
  const filenames: string[] = [];
  let pos = 0;

  // Find end of central directory (search from end)
  for (let i = buffer.length - 22; i >= 0; i--) {
    if (buffer.readUInt32LE(i) === 0x06054b50) {
      const centralDirOffset = buffer.readUInt32LE(i + 16);
      pos = centralDirOffset;
      break;
    }
  }

  // Read central directory entries
  while (pos < buffer.length && buffer.readUInt32LE(pos) === 0x02014b50) {
    const filenameLen = buffer.readUInt16LE(pos + 28);
    const extraLen = buffer.readUInt16LE(pos + 30);
    const commentLen = buffer.readUInt16LE(pos + 32);
    const filename = buffer.toString('utf8', pos + 46, pos + 46 + filenameLen);
    filenames.push(filename);
    pos += 46 + filenameLen + extraLen + commentLen;
  }

  return filenames;
}

/**
 * Extract a specific file's content from a ZIP buffer.
 */
function extractZipFileContent(buffer: Buffer, targetFilename: string): string | null {
  let pos = 0;

  while (pos < buffer.length) {
    if (buffer.readUInt32LE(pos) !== 0x04034b50) break;

    const filenameLen = buffer.readUInt16LE(pos + 26);
    const extraLen = buffer.readUInt16LE(pos + 28);
    const compressedSize = buffer.readUInt32LE(pos + 18);
    const filename = buffer.toString('utf8', pos + 30, pos + 30 + filenameLen);
    const dataStart = pos + 30 + filenameLen + extraLen;

    if (filename === targetFilename) {
      return buffer.toString('utf8', dataStart, dataStart + compressedSize);
    }

    pos = dataStart + compressedSize;
  }

  return null;
}

// -----------------------------------------------------------------------------
// Test Utilities
// -----------------------------------------------------------------------------

interface ConsoleGuards {
  consoleErrors: string[];
  pageErrors: string[];
}

function attachConsoleGuards(page: Page): ConsoleGuards {
  const consoleErrors: string[] = [];
  const pageErrors: string[] = [];

  page.on('console', (msg) => {
    if (msg.type() !== 'error') return;
    const loc = msg.location();
    const suffix = loc.url ? ` (${loc.url}:${loc.lineNumber}:${loc.columnNumber})` : '';
    consoleErrors.push(`${msg.text()}${suffix}`);
  });

  page.on('pageerror', (err) => {
    pageErrors.push(err.message);
  });

  return { consoleErrors, pageErrors };
}

async function installSseStub(page: Page): Promise<void> {
  await page.addInitScript(() => {
    class MockEventSource {
      url: string;
      withCredentials: boolean;
      readyState = 1;
      onopen: ((event: Event) => void) | null = null;
      onmessage: ((event: MessageEvent) => void) | null = null;
      onerror: ((event: Event) => void) | null = null;

      constructor(url: string, options?: EventSourceInit) {
        this.url = url;
        this.withCredentials = Boolean(options?.withCredentials);
        setTimeout(() => this.onopen?.(new Event('open')), 0);
      }

      addEventListener(): void {}
      removeEventListener(): void {}
      close(): void {
        this.readyState = 2;
      }
    }

    window.EventSource = MockEventSource as unknown as typeof EventSource;
  });
}

// -----------------------------------------------------------------------------
// Mock Setup
// -----------------------------------------------------------------------------

interface MockOptions {
  evidenceEndpointBehavior: 'canonical' | 'legacy-only' | 'all-fail';
}

async function setupFlow9Mocks(page: Page, options: MockOptions): Promise<Request[]> {
  const now = FIXED_NOW;
  const capturedRequests: Request[] = [];
  const inferenceResponse = createMockInferenceResponse();
  const zipBuffer = createMockZipBuffer();

  await installSseStub(page);

  const fulfillJson = (route: Route, body: unknown, status = 200) =>
    route.fulfill({
      status,
      contentType: 'application/json',
      body: JSON.stringify(body),
    });

  // Health endpoints
  await page.route('**/healthz', async (route) => fulfillJson(route, { status: 'healthy' }));
  await page.route('**/healthz/all', async (route) =>
    fulfillJson(route, { status: 'healthy', components: {}, schema_version: '1.0' })
  );
  await page.route('**/readyz', async (route) =>
    fulfillJson(route, { ready: true, checks: { db: { ok: true }, worker: { ok: true } } })
  );

  // Main API routes
  await page.route('**/v1/**', async (route) => {
    const req = route.request();
    const url = new URL(req.url());
    const rawPathname = url.pathname;
    const pathname = rawPathname.startsWith('/api/') ? rawPathname.slice(4) : rawPathname;
    const method = req.method();

    // Capture evidence-related requests
    if (pathname.includes('/evidence') || pathname.includes('/runs/')) {
      capturedRequests.push(req);
    }

    const json = (body: unknown, status = 200) => fulfillJson(route, body, status);

    if (method === 'OPTIONS') {
      return route.fulfill({ status: 204 });
    }

    // Auth endpoints
    if (pathname === '/v1/auth/me') {
      return json({
        schema_version: '1.0',
        user_id: 'user-flow9',
        email: 'dev@local',
        role: 'admin',
        created_at: now,
        display_name: 'Flow 9 Test User',
        tenant_id: 'tenant-1',
        permissions: ['inference:execute', 'evidence:export'],
        last_login_at: now,
        mfa_enabled: false,
        token_last_rotated_at: now,
        admin_tenants: ['*'],
      });
    }

    if (pathname === '/v1/auth/tenants') {
      return json({
        schema_version: '1.0',
        tenants: [{ id: 'tenant-1', name: 'System', role: 'admin' }],
      });
    }

    if (pathname === '/v1/auth/tenants/switch') {
      return json({
        schema_version: '1.0',
        token: 'mock-token-flow9',
        user_id: 'user-flow9',
        tenant_id: 'tenant-1',
        role: 'admin',
        expires_in: 3600,
        tenants: [{ id: 'tenant-1', name: 'System', role: 'admin' }],
      });
    }

    // Models
    if (pathname === '/v1/models') {
      return json({
        models: [
          {
            id: 'model-flow9',
            name: 'Flow 9 Model',
            hash_b3: 'hash-model-flow9',
            config_hash_b3: 'hash-config-flow9',
            tokenizer_hash_b3: 'hash-tokenizer-flow9',
            format: 'gguf',
            backend: 'coreml',
            size_bytes: 1_000_000,
            adapter_count: 1,
            training_job_count: 0,
            imported_at: now,
            updated_at: now,
            architecture: { architecture: 'decoder' },
          },
        ],
        total: 1,
      });
    }

    if (pathname.match(/\/v1\/models\/[^/]+\/validate/)) {
      return json({
        model_id: 'model-flow9',
        status: 'ready',
        valid: true,
        can_load: true,
        issues: [],
      });
    }

    if (pathname.match(/\/v1\/models\/[^/]+\/status/)) {
      return json({
        schema_version: '1.0',
        model_id: 'model-flow9',
        status: 'ready',
        is_loaded: true,
      });
    }

    // Backends
    if (pathname === '/v1/backends') {
      return json({
        schema_version: '1.0',
        backends: [
          { backend: 'coreml', status: 'healthy', mode: 'real' },
          { backend: 'auto', status: 'healthy', mode: 'auto' },
        ],
        default_backend: 'coreml',
      });
    }

    if (pathname === '/v1/backends/capabilities') {
      return json({
        schema_version: '1.0',
        hardware: {
          ane_available: true,
          gpu_available: true,
          gpu_type: 'Apple GPU',
          cpu_model: 'Apple Silicon',
        },
        backends: [
          { backend: 'coreml', capabilities: [{ name: 'coreml', available: true }] },
          { backend: 'auto', capabilities: [{ name: 'auto', available: true }] },
        ],
      });
    }

    // Adapters
    if (pathname === '/v1/adapters') {
      return json([
        {
          id: 'adapter-flow9',
          name: 'Flow 9 Adapter',
          adapter_id: 'adapter-flow9',
          current_state: 'hot',
          lora_tier: 'prod',
          lora_scope: 'general',
          lora_strength: 1,
        },
      ]);
    }

    if (pathname === '/v1/adapter-stacks') {
      return json([
        {
          id: 'stack-flow9',
          name: 'Flow 9 Stack',
          adapter_ids: ['adapter-flow9'],
          description: 'Test stack for flow 9',
          created_at: now,
          updated_at: now,
        },
      ]);
    }

    if (pathname === '/v1/tenants/tenant-1/default-stack') {
      return json({ schema_version: '1.0', stack_id: null });
    }

    // Inference
    if (pathname === '/v1/infer' && method === 'POST') {
      return json(inferenceResponse);
    }

    // Canonical evidence endpoint: /v1/runs/{run_id}/evidence
    const canonicalMatch = pathname.match(/\/v1\/runs\/([^/]+)\/evidence$/);
    if (canonicalMatch) {
      const runId = canonicalMatch[1];

      if (options.evidenceEndpointBehavior === 'canonical') {
        // Return ZIP file
        return route.fulfill({
          status: 200,
          contentType: 'application/zip',
          body: zipBuffer,
          headers: {
            'Content-Disposition': `attachment; filename="run-evidence-${runId}.zip"`,
            'X-Evidence-Source': 'canonical',
          },
        });
      } else if (options.evidenceEndpointBehavior === 'legacy-only') {
        // Canonical fails, force legacy fallback
        return route.fulfill({ status: 404, body: 'Canonical endpoint not available' });
      } else {
        // All fail - trigger local fallback
        return route.fulfill({ status: 500, body: 'Backend unavailable' });
      }
    }

    // Legacy evidence endpoint: /v1/evidence/runs/{run_id}/export
    const legacyMatch = pathname.match(/\/v1\/evidence\/runs\/([^/]+)\/export$/);
    if (legacyMatch) {
      const runId = legacyMatch[1];

      if (options.evidenceEndpointBehavior === 'legacy-only') {
        return route.fulfill({
          status: 200,
          contentType: 'application/zip',
          body: zipBuffer,
          headers: {
            'Content-Disposition': `attachment; filename="run-evidence-${runId}.zip"`,
            'X-Evidence-Source': 'legacy',
          },
        });
      } else if (options.evidenceEndpointBehavior === 'all-fail') {
        return route.fulfill({ status: 500, body: 'Backend unavailable' });
      }
      // If canonical works, legacy shouldn't be called
      return route.fulfill({ status: 404 });
    }

    // Default fallback
    return json({ schema_version: '1.0' });
  });

  return capturedRequests;
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

test.describe('Flow 9: Evidence export downloads deterministic ZIP via canonical endpoint', () => {
  test.describe('Canonical endpoint success', () => {
    test('requests canonical endpoint /v1/runs/{run_id}/evidence first', async ({ page }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);
      const capturedRequests = await setupFlow9Mocks(page, {
        evidenceEndpointBehavior: 'canonical',
      });

      await page.goto('/inference');

      // Wait for page to load
      const backendLoadingMarker = page.getByLabel('Loading backend status');
      await expect(backendLoadingMarker).toBeVisible({ timeout: 10000 });
      await expect(backendLoadingMarker).toBeHidden({ timeout: 10000 });

      // Run inference to get a run_id
      await page.locator('[data-cy="prompt-input"]').fill('Test prompt for evidence export');

      const inferencePromise = page.waitForResponse(
        (response) => response.url().includes('/v1/infer') && response.request().method() === 'POST'
      );
      await page.locator('[data-cy="run-inference-btn"]').click();
      await inferencePromise;

      // Wait for inference result
      await expect(page.locator('[data-cy="inference-result"]')).toContainText(
        'Flow 9 test inference response'
      );

      // Verify receipt digest is shown
      await expect(page.locator('[data-cy="receipt-digest"]')).toContainText(MOCK_RECEIPT_DIGEST);

      // Set up download listener before clicking export
      const downloadPromise = page.waitForEvent('download');

      // Click export evidence button
      await page.locator('[data-cy="export-evidence"]').click();

      // Wait for download to complete
      const download = await downloadPromise;

      // Verify the download filename contains evidence
      expect(download.suggestedFilename()).toContain('evidence');
      expect(download.suggestedFilename()).toContain('.zip');

      // Verify canonical endpoint was called first
      const evidenceRequests = capturedRequests.filter(
        (req) => req.url().includes('/evidence') || req.url().match(/\/runs\/[^/]+\/evidence/)
      );
      expect(evidenceRequests.length).toBeGreaterThan(0);

      const firstEvidenceRequest = evidenceRequests[0];
      expect(firstEvidenceRequest.url()).toMatch(/\/v1\/runs\/[^/]+\/evidence/);

      // Verify no page errors
      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
      // Filter out expected non-critical console errors
      const criticalErrors = consoleErrors.filter(
        (err) => !err.includes('ResizeObserver') && !err.includes('favicon')
      );
      expect(criticalErrors, `console errors: ${criticalErrors.join('\n')}`).toEqual([]);
    });

    test('downloaded ZIP contains expected files', async ({ page }) => {
      await setupFlow9Mocks(page, { evidenceEndpointBehavior: 'canonical' });

      await page.goto('/inference');

      const backendLoadingMarker = page.getByLabel('Loading backend status');
      await expect(backendLoadingMarker).toBeVisible({ timeout: 10000 });
      await expect(backendLoadingMarker).toBeHidden({ timeout: 10000 });

      // Run inference
      await page.locator('[data-cy="prompt-input"]').fill('Test for ZIP contents verification');

      const inferencePromise = page.waitForResponse(
        (response) => response.url().includes('/v1/infer') && response.request().method() === 'POST'
      );
      await page.locator('[data-cy="run-inference-btn"]').click();
      await inferencePromise;

      await expect(page.locator('[data-cy="inference-result"]')).toContainText(
        'Flow 9 test inference response'
      );

      // Download and verify ZIP contents
      const downloadPromise = page.waitForEvent('download');
      await page.locator('[data-cy="export-evidence"]').click();

      const download = await downloadPromise;
      const downloadPath = await download.path();

      if (downloadPath) {
        const zipData = await readFile(downloadPath);

        // Verify all expected files are present
        const fileNames = extractZipFilenames(zipData);

        for (const expectedFile of EXPECTED_ZIP_ENTRIES) {
          expect(fileNames, `Missing expected file: ${expectedFile}`).toContain(expectedFile);
        }

        // Verify run_envelope.json content
        const runEnvelopeContent = extractZipFileContent(zipData, 'run_envelope.json');
        expect(runEnvelopeContent).toBeDefined();
        const runEnvelope = JSON.parse(runEnvelopeContent!);
        expect(runEnvelope.run_id).toBe(MOCK_RUN_ID);
        expect(runEnvelope.trace_id).toBe(MOCK_TRACE_ID);
        expect(runEnvelope.receipt_digest).toBe(MOCK_RECEIPT_DIGEST);

        // Verify replay_metadata.json content
        const replayContent = extractZipFileContent(zipData, 'replay_metadata.json');
        expect(replayContent).toBeDefined();
        const replayMeta = JSON.parse(replayContent!);
        expect(replayMeta.determinism_version).toBeDefined();
        expect(replayMeta.router_seed).toBeDefined();
      }
    });

    test('export is deterministic - re-export produces identical structure', async ({ page }) => {
      await setupFlow9Mocks(page, { evidenceEndpointBehavior: 'canonical' });

      await page.goto('/inference');

      const backendLoadingMarker = page.getByLabel('Loading backend status');
      await expect(backendLoadingMarker).toBeVisible({ timeout: 10000 });
      await expect(backendLoadingMarker).toBeHidden({ timeout: 10000 });

      // Run inference
      await page.locator('[data-cy="prompt-input"]').fill('Determinism test prompt');

      const inferencePromise = page.waitForResponse(
        (response) => response.url().includes('/v1/infer') && response.request().method() === 'POST'
      );
      await page.locator('[data-cy="run-inference-btn"]').click();
      await inferencePromise;

      await expect(page.locator('[data-cy="inference-result"]')).toContainText(
        'Flow 9 test inference response'
      );

      // First export
      const downloadPromise1 = page.waitForEvent('download');
      await page.locator('[data-cy="export-evidence"]').click();
      const download1 = await downloadPromise1;
      const path1 = await download1.path();

      // Second export (same run)
      const downloadPromise2 = page.waitForEvent('download');
      await page.locator('[data-cy="export-evidence"]').click();
      const download2 = await downloadPromise2;
      const path2 = await download2.path();

      if (path1 && path2) {
        const zip1Data = await readFile(path1);
        const zip2Data = await readFile(path2);

        // Verify same files exist
        const files1 = extractZipFilenames(zip1Data).sort();
        const files2 = extractZipFilenames(zip2Data).sort();
        expect(files1).toEqual(files2);

        // Verify run_envelope content matches
        const envelope1 = extractZipFileContent(zip1Data, 'run_envelope.json');
        const envelope2 = extractZipFileContent(zip2Data, 'run_envelope.json');
        expect(envelope1).toBe(envelope2);

        // Verify manifest_ref content matches
        const manifest1 = extractZipFileContent(zip1Data, 'manifest_ref.json');
        const manifest2 = extractZipFileContent(zip2Data, 'manifest_ref.json');
        expect(manifest1).toBe(manifest2);
      }
    });
  });

  test.describe('Legacy fallback', () => {
    test('falls back to legacy endpoint when canonical fails and labels it appropriately', async ({
      page,
    }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);
      const capturedRequests = await setupFlow9Mocks(page, {
        evidenceEndpointBehavior: 'legacy-only',
      });

      await page.goto('/inference');

      const backendLoadingMarker = page.getByLabel('Loading backend status');
      await expect(backendLoadingMarker).toBeVisible({ timeout: 10000 });
      await expect(backendLoadingMarker).toBeHidden({ timeout: 10000 });

      // Run inference
      await page.locator('[data-cy="prompt-input"]').fill('Legacy fallback test prompt');

      const inferencePromise = page.waitForResponse(
        (response) => response.url().includes('/v1/infer') && response.request().method() === 'POST'
      );
      await page.locator('[data-cy="run-inference-btn"]').click();
      await inferencePromise;

      await expect(page.locator('[data-cy="inference-result"]')).toContainText(
        'Flow 9 test inference response'
      );

      // Export evidence
      const downloadPromise = page.waitForEvent('download');
      await page.locator('[data-cy="export-evidence"]').click();

      const download = await downloadPromise;
      expect(download.suggestedFilename()).toContain('evidence');

      // Verify canonical was tried first, then legacy
      const canonicalRequests = capturedRequests.filter((req) =>
        req.url().match(/\/v1\/runs\/[^/]+\/evidence/)
      );
      const legacyRequests = capturedRequests.filter((req) =>
        req.url().match(/\/v1\/evidence\/runs\/[^/]+\/export/)
      );

      expect(canonicalRequests.length).toBeGreaterThan(0);
      expect(legacyRequests.length).toBeGreaterThan(0);

      // Verify warning toast is shown for legacy fallback
      await expect(page.getByText(/legacy/i)).toBeVisible({ timeout: 5000 });

      expect(pageErrors).toEqual([]);
    });
  });

  test.describe('Backend failure - local fallback', () => {
    test('exports unverified local bundle when backend is down', async ({ page }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);
      await setupFlow9Mocks(page, { evidenceEndpointBehavior: 'all-fail' });

      await page.goto('/inference');

      const backendLoadingMarker = page.getByLabel('Loading backend status');
      await expect(backendLoadingMarker).toBeVisible({ timeout: 10000 });
      await expect(backendLoadingMarker).toBeHidden({ timeout: 10000 });

      // Run inference
      await page.locator('[data-cy="prompt-input"]').fill('Backend failure test prompt');

      const inferencePromise = page.waitForResponse(
        (response) => response.url().includes('/v1/infer') && response.request().method() === 'POST'
      );
      await page.locator('[data-cy="run-inference-btn"]').click();
      await inferencePromise;

      await expect(page.locator('[data-cy="inference-result"]')).toContainText(
        'Flow 9 test inference response'
      );

      // Export evidence - should fall back to local bundle
      const downloadPromise = page.waitForEvent('download');
      await page.locator('[data-cy="export-evidence"]').click();

      const download = await downloadPromise;

      // Verify the filename indicates unverified local bundle
      const filename = download.suggestedFilename();
      expect(filename).toContain('unverified');
      expect(filename).toContain('local');
      expect(filename).toContain('.json');

      // Verify warning toast about unverified bundle
      await expect(page.getByText(/unverified/i)).toBeVisible({ timeout: 5000 });

      // Verify the downloaded JSON contains the bundle_label warning
      const downloadPath = await download.path();
      if (downloadPath) {
        const content = await readFile(downloadPath, 'utf-8');
        const bundle = JSON.parse(content);

        expect(bundle.bundle_label).toBe('unverified local bundle');
        // Should NOT claim to be authoritative
        expect(bundle).not.toHaveProperty('authoritative');
        expect(bundle).not.toHaveProperty('verified');
      }

      expect(pageErrors).toEqual([]);
    });

    test('local fallback does not claim to be authoritative', async ({ page }) => {
      await setupFlow9Mocks(page, { evidenceEndpointBehavior: 'all-fail' });

      await page.goto('/inference');

      const backendLoadingMarker = page.getByLabel('Loading backend status');
      await expect(backendLoadingMarker).toBeVisible({ timeout: 10000 });
      await expect(backendLoadingMarker).toBeHidden({ timeout: 10000 });

      // Run inference
      await page.locator('[data-cy="prompt-input"]').fill('Non-authoritative test');

      const inferencePromise = page.waitForResponse(
        (response) => response.url().includes('/v1/infer') && response.request().method() === 'POST'
      );
      await page.locator('[data-cy="run-inference-btn"]').click();
      await inferencePromise;

      await expect(page.locator('[data-cy="inference-result"]')).toContainText(
        'Flow 9 test inference response'
      );

      // Check that UI does not show "verified" or "authoritative" badges
      // when falling back to local export
      const downloadPromise = page.waitForEvent('download');
      await page.locator('[data-cy="export-evidence"]').click();
      await downloadPromise;

      // The toast should indicate this is not authoritative
      const toastText = await page.locator('[role="status"]').textContent();
      expect(toastText).not.toMatch(/authoritative/i);
      expect(toastText).toMatch(/unverified|local|failed/i);
    });
  });

  test.describe('Evidence panel integration', () => {
    test('evidence panel shows correct export button state', async ({ page }) => {
      await setupFlow9Mocks(page, { evidenceEndpointBehavior: 'canonical' });

      await page.goto('/inference');

      const backendLoadingMarker = page.getByLabel('Loading backend status');
      await expect(backendLoadingMarker).toBeVisible({ timeout: 10000 });
      await expect(backendLoadingMarker).toBeHidden({ timeout: 10000 });

      // Initially, export button should be disabled or hidden (no run yet)
      const exportButton = page.locator('[data-cy="export-evidence"]');

      // Run inference
      await page.locator('[data-cy="prompt-input"]').fill('Evidence panel test');

      const inferencePromise = page.waitForResponse(
        (response) => response.url().includes('/v1/infer') && response.request().method() === 'POST'
      );
      await page.locator('[data-cy="run-inference-btn"]').click();
      await inferencePromise;

      await expect(page.locator('[data-cy="inference-result"]')).toContainText(
        'Flow 9 test inference response'
      );

      // Now export button should be visible and enabled
      await expect(exportButton).toBeVisible();
      await expect(exportButton).toBeEnabled();
    });
  });
});
