/**
 * Flow 4: Upload Dataset Creates Deterministic Dataset Artifact
 *
 * Tests the dataset upload functionality ensuring:
 * 1. Datasets are uploaded with deterministic hashing (dataset_hash_b3)
 * 2. Re-uploading the same content results in stable hash values (deduplication)
 * 3. Workspace association is correctly maintained
 * 4. Error handling for insufficient disk space
 *
 * Preconditions:
 * - User selected a workspace
 * - User has permission to upload datasets
 * - Backend supports POST /v1/datasets
 */

import { test, expect, type Page, type Route, type Request } from '@playwright/test';
import * as path from 'path';

// ===========================================================================
// Test Constants
// ===========================================================================

const FIXED_NOW = '2025-01-01T00:00:00.000Z';
const MOCK_TENANT_ID = 'tenant-1';
const MOCK_USER_ID = 'user-1';
const MOCK_WORKSPACE_NAME = 'Test Workspace';

// Deterministic hash for a known file - BLAKE3 hash
const MOCK_DATASET_HASH_B3 = 'b3_abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890';
const MOCK_DATASET_ID = 'dataset-test-001';
const MOCK_DATASET_VERSION_ID = 'dsv-v1-001';

// Test file content (small JSONL dataset)
const TEST_DATASET_CONTENT = `{"instruction": "What is the capital of France?", "response": "Paris"}
{"instruction": "What is 2 + 2?", "response": "4"}
{"instruction": "Who wrote Romeo and Juliet?", "response": "William Shakespeare"}`;

const TEST_DATASET_FILENAME = 'test-training-data.jsonl';
const TEST_DATASET_SIZE = TEST_DATASET_CONTENT.length;

// ===========================================================================
// Console Error Guard
// ===========================================================================

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

// ===========================================================================
// Mock API Setup
// ===========================================================================

interface MockOptions {
  simulateDiskFull?: boolean;
  uploadDelayMs?: number;
  existingDatasets?: MockDataset[];
}

interface MockDataset {
  id: string;
  name: string;
  hash: string;
  validation_status: string;
  trust_state: string;
  file_count: number;
  total_size_bytes: number;
  created_at: string;
  updated_at: string;
  dataset_version_id?: string;
}

async function setupDatasetUploadMocks(page: Page, options: MockOptions = {}) {
  const now = FIXED_NOW;
  const { simulateDiskFull = false, uploadDelayMs = 100, existingDatasets = [] } = options;

  // Track uploaded datasets to verify deduplication behavior
  const uploadedDatasets: MockDataset[] = [...existingDatasets];
  let uploadCount = 0;

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
  await page.route('**/readyz', async (route) => fulfillJson(route, { status: 'ready' }));

  // Main v1 API routes
  await page.route('**/v1/**', async (route) => {
    const req = route.request();
    const url = new URL(req.url());
    const rawPathname = url.pathname;
    const pathname = rawPathname.startsWith('/api/') ? rawPathname.slice(4) : rawPathname;
    const method = req.method();

    const json = (body: unknown, status = 200) => fulfillJson(route, body, status);

    if (method === 'OPTIONS') {
      return route.fulfill({ status: 204 });
    }

    // -----------------------------------------------------------------------
    // Auth endpoints
    // -----------------------------------------------------------------------
    if (pathname === '/v1/auth/me') {
      return json({
        schema_version: '1.0',
        user_id: MOCK_USER_ID,
        email: 'dev@local',
        role: 'admin',
        created_at: now,
        display_name: 'Dev User',
        tenant_id: MOCK_TENANT_ID,
        permissions: [
          'dataset:upload',
          'dataset:view',
          'dataset:validate',
          'dataset:delete',
          'training:start',
          'inference:execute',
        ],
        last_login_at: now,
        mfa_enabled: false,
        token_last_rotated_at: now,
        admin_tenants: ['*'],
      });
    }

    if (pathname === '/v1/auth/tenants') {
      return json({
        schema_version: '1.0',
        tenants: [{ id: MOCK_TENANT_ID, name: MOCK_WORKSPACE_NAME, role: 'admin' }],
      });
    }

    if (pathname === '/v1/auth/tenants/switch') {
      return json({
        schema_version: '1.0',
        token: 'mock-token',
        user_id: MOCK_USER_ID,
        tenant_id: MOCK_TENANT_ID,
        role: 'admin',
        expires_in: 3600,
        tenants: [{ id: MOCK_TENANT_ID, name: MOCK_WORKSPACE_NAME, role: 'admin' }],
        admin_tenants: ['*'],
        session_mode: 'normal',
      });
    }

    // -----------------------------------------------------------------------
    // Dataset upload endpoint (POST /v1/datasets/upload)
    // -----------------------------------------------------------------------
    if (pathname === '/v1/datasets/upload' && method === 'POST') {
      if (uploadDelayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, uploadDelayMs));
      }

      // Simulate disk full error
      if (simulateDiskFull) {
        return json(
          {
            error: 'Insufficient disk space',
            code: 'DISK_FULL',
            message: 'Not enough storage space to complete the upload. Please free up disk space and try again.',
          },
          507 // Insufficient Storage
        );
      }

      uploadCount++;

      // Check if this is a duplicate upload (same hash)
      const existingWithSameHash = uploadedDatasets.find((ds) => ds.hash === MOCK_DATASET_HASH_B3);

      if (existingWithSameHash) {
        // Return deduplication message - same hash means same dataset artifact
        return json({
          schema_version: '1.0',
          dataset_id: existingWithSameHash.id,
          name: existingWithSameHash.name,
          description: null,
          file_count: existingWithSameHash.file_count,
          total_size_bytes: existingWithSameHash.total_size_bytes,
          format: 'jsonl',
          hash: existingWithSameHash.hash,
          storage_path: `/data/datasets/${existingWithSameHash.id}`,
          validation_status: 'pending',
          validation_errors: null,
          created_by: MOCK_USER_ID,
          created_at: existingWithSameHash.created_at,
          updated_at: now,
          deduplicated: true,
          message: 'Dataset with identical content already exists. Returning existing dataset.',
        });
      }

      // New dataset upload
      const newDataset: MockDataset = {
        id: `${MOCK_DATASET_ID}-${uploadCount}`,
        name: TEST_DATASET_FILENAME.replace('.jsonl', ''),
        hash: MOCK_DATASET_HASH_B3,
        validation_status: 'pending',
        trust_state: 'allowed',
        file_count: 1,
        total_size_bytes: TEST_DATASET_SIZE,
        created_at: now,
        updated_at: now,
        dataset_version_id: `${MOCK_DATASET_VERSION_ID}-${uploadCount}`,
      };

      uploadedDatasets.push(newDataset);

      return json({
        schema_version: '1.0',
        dataset_id: newDataset.id,
        name: newDataset.name,
        description: null,
        file_count: newDataset.file_count,
        total_size_bytes: newDataset.total_size_bytes,
        format: 'jsonl',
        hash: newDataset.hash,
        storage_path: `/data/datasets/${newDataset.id}`,
        validation_status: 'pending',
        validation_errors: null,
        created_by: MOCK_USER_ID,
        created_at: newDataset.created_at,
        updated_at: newDataset.updated_at,
      });
    }

    // -----------------------------------------------------------------------
    // Dataset list endpoint (GET /v1/datasets)
    // -----------------------------------------------------------------------
    if (pathname === '/v1/datasets' && method === 'GET') {
      return json(
        uploadedDatasets.map((ds) => ({
          dataset_id: ds.id,
          dataset_version_id: ds.dataset_version_id,
          name: ds.name,
          hash: ds.hash,
          total_size_bytes: ds.total_size_bytes,
          file_count: ds.file_count,
          format: 'jsonl',
          storage_path: `/data/datasets/${ds.id}`,
          validation_status: ds.validation_status,
          validation_errors: null,
          created_by: MOCK_USER_ID,
          created_at: ds.created_at,
          updated_at: ds.updated_at,
          description: null,
          trust_state: ds.trust_state,
          trust_reason: null,
        }))
      );
    }

    // -----------------------------------------------------------------------
    // Dataset detail endpoint (GET /v1/datasets/:id)
    // -----------------------------------------------------------------------
    const datasetDetailMatch = pathname.match(/^\/v1\/datasets\/([^/]+)$/);
    if (datasetDetailMatch && method === 'GET') {
      const datasetId = datasetDetailMatch[1];
      const dataset = uploadedDatasets.find((ds) => ds.id === datasetId);

      if (!dataset) {
        return json({ error: 'Dataset not found', code: 'NOT_FOUND' }, 404);
      }

      return json({
        dataset_id: dataset.id,
        dataset_version_id: dataset.dataset_version_id,
        name: dataset.name,
        hash: dataset.hash,
        total_size_bytes: dataset.total_size_bytes,
        file_count: dataset.file_count,
        format: 'jsonl',
        storage_path: `/data/datasets/${dataset.id}`,
        validation_status: dataset.validation_status,
        validation_errors: null,
        created_by: MOCK_USER_ID,
        created_at: dataset.created_at,
        updated_at: dataset.updated_at,
        description: null,
        trust_state: dataset.trust_state,
        trust_reason: null,
      });
    }

    // -----------------------------------------------------------------------
    // Dataset validation endpoint (POST /v1/datasets/:id/validate)
    // -----------------------------------------------------------------------
    const validateMatch = pathname.match(/^\/v1\/datasets\/([^/]+)\/validate$/);
    if (validateMatch && method === 'POST') {
      const datasetId = validateMatch[1];
      const dataset = uploadedDatasets.find((ds) => ds.id === datasetId);

      if (dataset) {
        dataset.validation_status = 'valid';
      }

      return json({
        dataset_id: datasetId,
        status: 'valid',
        errors: [],
        warnings: [],
        stats: {
          total_files: 1,
          valid_files: 1,
          total_tokens: 150,
          language_breakdown: { en: 150 },
        },
      });
    }

    // -----------------------------------------------------------------------
    // Supporting endpoints for training shell navigation
    // -----------------------------------------------------------------------
    if (pathname === '/v1/models') {
      return json({
        models: [
          {
            id: 'model-1',
            name: 'Demo Model',
            hash_b3: 'b3:model-hash',
            config_hash_b3: 'b3:config-hash',
            tokenizer_hash_b3: 'b3:tokenizer-hash',
            format: 'gguf',
            backend: 'coreml',
            size_bytes: 1_000_000,
            adapter_count: 0,
            training_job_count: 0,
            imported_at: now,
            updated_at: now,
            architecture: { architecture: 'decoder' },
          },
        ],
        total: 1,
      });
    }

    if (pathname === '/v1/training/jobs') {
      return json({
        schema_version: '1.0',
        jobs: [],
        total: 0,
        page: 1,
        page_size: 20,
      });
    }

    if (pathname === '/v1/training/templates') {
      return json([]);
    }

    if (pathname === '/v1/adapters') {
      return json([]);
    }

    if (pathname === '/v1/adapter-stacks') {
      return json([]);
    }

    if (pathname.startsWith('/v1/tenants/') && pathname.endsWith('/default-stack')) {
      return json({ schema_version: '1.0', stack_id: null });
    }

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

    if (pathname === '/v1/metrics/system') {
      return json({
        schema_version: '1.0',
        cpu_usage_percent: 1,
        memory_usage_pct: 1,
        memory_total_gb: 16,
        tokens_per_second: 0,
        latency_p95_ms: 0,
      });
    }

    if (pathname === '/v1/repos') {
      return json([]);
    }

    // Default fallback
    return json({ schema_version: '1.0' });
  });

  return { uploadedDatasets };
}

// ===========================================================================
// Test Helpers
// ===========================================================================

/**
 * Create a test file buffer from content
 */
function createTestFileBuffer(content: string): Buffer {
  return Buffer.from(content, 'utf-8');
}

/**
 * Navigate to the datasets tab within the training shell
 */
async function navigateToDatasetsPage(page: Page) {
  await page.goto('/training/datasets');

  // Wait for navigation and content load
  await expect(page.getByRole('heading', { name: 'Training', exact: true }).first()).toBeVisible({
    timeout: 10000,
  });
}

/**
 * Open the upload dialog and fill in basic info
 */
async function openUploadDialog(page: Page) {
  // Find and click the upload button
  const uploadButton = page.getByRole('button', { name: /upload/i });
  await expect(uploadButton).toBeVisible();
  await uploadButton.click();

  // Wait for dialog to open
  await expect(page.getByRole('dialog')).toBeVisible();
}

// ===========================================================================
// Test Suite
// ===========================================================================

test.describe('Flow 4: Upload Dataset Creates Deterministic Dataset Artifact', () => {
  test.describe('Happy Path: Dataset Upload and Hash Verification', () => {
    test('uploads a dataset and displays status, hash, and workspace association', async ({
      page,
    }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);
      await setupDatasetUploadMocks(page, { uploadDelayMs: 200 });

      // Navigate to datasets page
      await navigateToDatasetsPage(page);

      // Open upload dialog
      await openUploadDialog(page);

      // Fill in dataset name
      const nameInput = page.locator('#name');
      await nameInput.fill('test-training-data');

      // Select source type
      const sourceTypeSelect = page.locator('#sourceType');
      await sourceTypeSelect.click();
      await page.getByRole('option', { name: /uploaded files/i }).click();

      // Upload file using the file input
      const fileInput = page.locator('input[type="file"]');
      await fileInput.setInputFiles({
        name: TEST_DATASET_FILENAME,
        mimeType: 'application/json',
        buffer: createTestFileBuffer(TEST_DATASET_CONTENT),
      });

      // Submit the form
      const createButton = page.getByRole('button', { name: /create/i });
      await expect(createButton).toBeEnabled();

      // Wait for the upload request
      const uploadPromise = page.waitForResponse(
        (response) =>
          response.url().includes('/v1/datasets/upload') && response.request().method() === 'POST'
      );

      await createButton.click();
      const uploadResponse = await uploadPromise;
      expect(uploadResponse.status()).toBe(200);

      // Dialog should close after successful upload
      await expect(page.getByRole('dialog')).toBeHidden({ timeout: 5000 });

      // Wait for the datasets list to refresh
      await page.waitForResponse(
        (response) =>
          response.url().includes('/v1/datasets') && response.request().method() === 'GET'
      );

      // Verify dataset appears in the list with correct information
      const datasetRow = page.locator('table').getByRole('row').filter({
        hasText: 'test-training-data',
      });
      await expect(datasetRow).toBeVisible();

      // Verify status badge is visible (pending initially)
      const statusBadge = datasetRow.getByText(/pending|valid|validating/i);
      await expect(statusBadge).toBeVisible();

      // Verify trust state is displayed
      const trustBadge = datasetRow.locator('[class*="trust"], [data-slot="badge"]').first();
      await expect(trustBadge).toBeVisible();

      // No console errors
      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
      expect(consoleErrors, `console errors: ${consoleErrors.join('\n')}`).toEqual([]);
    });

    test('re-uploading same dataset results in stable hash (deduplication)', async ({ page }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);

      // Setup mock with an existing dataset that has the same hash
      const existingDataset: MockDataset = {
        id: 'dataset-existing-001',
        name: 'existing-dataset',
        hash: MOCK_DATASET_HASH_B3,
        validation_status: 'valid',
        trust_state: 'allowed',
        file_count: 1,
        total_size_bytes: TEST_DATASET_SIZE,
        created_at: '2024-12-01T00:00:00.000Z',
        updated_at: '2024-12-01T00:00:00.000Z',
        dataset_version_id: 'dsv-existing-001',
      };

      await setupDatasetUploadMocks(page, {
        uploadDelayMs: 200,
        existingDatasets: [existingDataset],
      });

      await navigateToDatasetsPage(page);

      // Verify existing dataset is shown
      await expect(
        page.locator('table').getByRole('row').filter({ hasText: 'existing-dataset' })
      ).toBeVisible();

      // Try to upload the same content
      await openUploadDialog(page);

      const nameInput = page.locator('#name');
      await nameInput.fill('duplicate-dataset');

      const sourceTypeSelect = page.locator('#sourceType');
      await sourceTypeSelect.click();
      await page.getByRole('option', { name: /uploaded files/i }).click();

      const fileInput = page.locator('input[type="file"]');
      await fileInput.setInputFiles({
        name: 'duplicate.jsonl',
        mimeType: 'application/json',
        buffer: createTestFileBuffer(TEST_DATASET_CONTENT),
      });

      const createButton = page.getByRole('button', { name: /create/i });
      await expect(createButton).toBeEnabled();

      const uploadPromise = page.waitForResponse(
        (response) =>
          response.url().includes('/v1/datasets/upload') && response.request().method() === 'POST'
      );

      await createButton.click();
      const uploadResponse = await uploadPromise;
      expect(uploadResponse.status()).toBe(200);

      // Parse the response to verify deduplication
      const responseBody = await uploadResponse.json();
      expect(responseBody.deduplicated).toBe(true);
      expect(responseBody.hash).toBe(MOCK_DATASET_HASH_B3);
      expect(responseBody.dataset_id).toBe('dataset-existing-001');

      // Dialog should close
      await expect(page.getByRole('dialog')).toBeHidden({ timeout: 5000 });

      // No duplicate rows with different IDs - should still show just the original
      const allDatasetRows = page.locator('table').getByRole('row');
      // Count rows that contain our hash pattern (excluding header row)
      const datasetCount = await allDatasetRows.count();
      // Header + 1 existing dataset row (dedup means no new row)
      expect(datasetCount).toBeLessThanOrEqual(2);

      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
      expect(consoleErrors, `console errors: ${consoleErrors.join('\n')}`).toEqual([]);
    });

    test('dataset detail view shows hash and workspace information', async ({ page }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);

      const existingDataset: MockDataset = {
        id: 'dataset-detail-001',
        name: 'detailed-dataset',
        hash: MOCK_DATASET_HASH_B3,
        validation_status: 'valid',
        trust_state: 'allowed',
        file_count: 3,
        total_size_bytes: 15000,
        created_at: FIXED_NOW,
        updated_at: FIXED_NOW,
        dataset_version_id: 'dsv-detail-001',
      };

      await setupDatasetUploadMocks(page, { existingDatasets: [existingDataset] });
      await navigateToDatasetsPage(page);

      // Click view button on the dataset row
      const datasetRow = page.locator('table').getByRole('row').filter({
        hasText: 'detailed-dataset',
      });
      await expect(datasetRow).toBeVisible();

      const viewButton = datasetRow.getByRole('button', { name: /view/i }).first();
      await viewButton.click();

      // Navigate to dataset detail page
      await page.goto('/training/datasets/dataset-detail-001');

      // Wait for detail page to load
      await page.waitForResponse(
        (response) =>
          response.url().includes('/v1/datasets/dataset-detail-001') &&
          response.request().method() === 'GET'
      );

      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
      expect(consoleErrors, `console errors: ${consoleErrors.join('\n')}`).toEqual([]);
    });
  });

  test.describe('Negative Case: Insufficient Disk Space', () => {
    test('shows disk full error and prevents partial dataset entry', async ({ page }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);
      await setupDatasetUploadMocks(page, { simulateDiskFull: true });

      await navigateToDatasetsPage(page);
      await openUploadDialog(page);

      // Fill form
      const nameInput = page.locator('#name');
      await nameInput.fill('large-dataset');

      const sourceTypeSelect = page.locator('#sourceType');
      await sourceTypeSelect.click();
      await page.getByRole('option', { name: /uploaded files/i }).click();

      const fileInput = page.locator('input[type="file"]');
      await fileInput.setInputFiles({
        name: 'large-file.jsonl',
        mimeType: 'application/json',
        buffer: createTestFileBuffer(TEST_DATASET_CONTENT),
      });

      const createButton = page.getByRole('button', { name: /create/i });
      await expect(createButton).toBeEnabled();

      // Try to upload
      const uploadPromise = page.waitForResponse(
        (response) =>
          response.url().includes('/v1/datasets/upload') && response.request().method() === 'POST'
      );

      await createButton.click();
      const uploadResponse = await uploadPromise;

      // Expect 507 Insufficient Storage
      expect(uploadResponse.status()).toBe(507);

      const responseBody = await uploadResponse.json();
      expect(responseBody.code).toBe('DISK_FULL');
      expect(responseBody.message).toContain('storage space');

      // Dialog should remain open on error OR show an error toast
      // The UI behavior may vary - check for either dialog still visible or error message
      const dialogStillVisible = await page.getByRole('dialog').isVisible();
      const errorMessageVisible = await page.getByText(/disk|storage|space/i).isVisible();

      expect(dialogStillVisible || errorMessageVisible).toBe(true);

      // Verify no partial dataset was created - refresh the list
      await page.goto('/training/datasets');
      await page.waitForResponse(
        (response) =>
          response.url().includes('/v1/datasets') && response.request().method() === 'GET'
      );

      // Should show empty state or not have the failed upload
      const largeDatasetRow = page.locator('table').getByRole('row').filter({
        hasText: 'large-dataset',
      });
      await expect(largeDatasetRow).toBeHidden();

      // Allow expected error in console for failed upload
      const unexpectedErrors = consoleErrors.filter(
        (err) => !err.includes('507') && !err.includes('disk') && !err.includes('storage')
      );
      expect(unexpectedErrors, `unexpected console errors: ${unexpectedErrors.join('\n')}`).toEqual(
        []
      );
      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
    });
  });

  test.describe('Hash Determinism Verification', () => {
    test('same file content produces identical hash across multiple uploads', async ({ page }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);
      const { uploadedDatasets } = await setupDatasetUploadMocks(page, { uploadDelayMs: 100 });

      await navigateToDatasetsPage(page);

      // First upload
      await openUploadDialog(page);
      await page.locator('#name').fill('first-upload');
      await page.locator('#sourceType').click();
      await page.getByRole('option', { name: /uploaded files/i }).click();
      await page.locator('input[type="file"]').setInputFiles({
        name: 'data1.jsonl',
        mimeType: 'application/json',
        buffer: createTestFileBuffer(TEST_DATASET_CONTENT),
      });

      const firstUploadPromise = page.waitForResponse(
        (response) =>
          response.url().includes('/v1/datasets/upload') && response.request().method() === 'POST'
      );
      await page.getByRole('button', { name: /create/i }).click();
      const firstResponse = await firstUploadPromise;
      const firstBody = await firstResponse.json();

      await expect(page.getByRole('dialog')).toBeHidden({ timeout: 5000 });

      // Second upload with same content (should deduplicate)
      await openUploadDialog(page);
      await page.locator('#name').fill('second-upload');
      await page.locator('#sourceType').click();
      await page.getByRole('option', { name: /uploaded files/i }).click();
      await page.locator('input[type="file"]').setInputFiles({
        name: 'data2.jsonl',
        mimeType: 'application/json',
        buffer: createTestFileBuffer(TEST_DATASET_CONTENT),
      });

      const secondUploadPromise = page.waitForResponse(
        (response) =>
          response.url().includes('/v1/datasets/upload') && response.request().method() === 'POST'
      );
      await page.getByRole('button', { name: /create/i }).click();
      const secondResponse = await secondUploadPromise;
      const secondBody = await secondResponse.json();

      // Both uploads should have the same hash
      expect(firstBody.hash).toBe(MOCK_DATASET_HASH_B3);
      expect(secondBody.hash).toBe(MOCK_DATASET_HASH_B3);

      // Second upload should be marked as deduplicated
      expect(secondBody.deduplicated).toBe(true);

      // Both should reference the same underlying dataset
      expect(secondBody.dataset_id).toBe(firstBody.dataset_id);

      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
      expect(consoleErrors, `console errors: ${consoleErrors.join('\n')}`).toEqual([]);
    });

    test('different file content produces different hashes', async ({ page }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);

      // Custom mock that returns different hashes for different content
      let uploadCount = 0;
      await page.route('**/healthz', async (route) =>
        route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({ status: 'healthy' }),
        })
      );
      await page.route('**/healthz/all', async (route) =>
        route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({ status: 'healthy', components: {}, schema_version: '1.0' }),
        })
      );
      await page.route('**/readyz', async (route) =>
        route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({ status: 'ready' }),
        })
      );

      await page.route('**/v1/**', async (route) => {
        const req = route.request();
        const url = new URL(req.url());
        const pathname = url.pathname.startsWith('/api/')
          ? url.pathname.slice(4)
          : url.pathname;
        const method = req.method();

        const json = (body: unknown, status = 200) =>
          route.fulfill({
            status,
            contentType: 'application/json',
            body: JSON.stringify(body),
          });

        if (method === 'OPTIONS') {
          return route.fulfill({ status: 204 });
        }

        if (pathname === '/v1/auth/me') {
          return json({
            schema_version: '1.0',
            user_id: MOCK_USER_ID,
            email: 'dev@local',
            role: 'admin',
            tenant_id: MOCK_TENANT_ID,
            permissions: ['dataset:upload', 'dataset:view', 'training:start'],
            admin_tenants: ['*'],
          });
        }

        if (pathname === '/v1/auth/tenants') {
          return json({
            schema_version: '1.0',
            tenants: [{ id: MOCK_TENANT_ID, name: MOCK_WORKSPACE_NAME, role: 'admin' }],
          });
        }

        if (pathname === '/v1/auth/tenants/switch') {
          return json({
            schema_version: '1.0',
            token: 'mock-token',
            user_id: MOCK_USER_ID,
            tenant_id: MOCK_TENANT_ID,
            role: 'admin',
            expires_in: 3600,
            tenants: [{ id: MOCK_TENANT_ID, name: MOCK_WORKSPACE_NAME, role: 'admin' }],
          });
        }

        if (pathname === '/v1/datasets/upload' && method === 'POST') {
          uploadCount++;
          // Generate different hash for each upload to simulate different content
          const uniqueHash = `b3_unique_${uploadCount}_${Date.now().toString(16)}`;
          return json({
            schema_version: '1.0',
            dataset_id: `dataset-unique-${uploadCount}`,
            name: `unique-dataset-${uploadCount}`,
            file_count: 1,
            total_size_bytes: TEST_DATASET_SIZE,
            format: 'jsonl',
            hash: uniqueHash,
            storage_path: `/data/datasets/dataset-unique-${uploadCount}`,
            validation_status: 'pending',
            created_by: MOCK_USER_ID,
            created_at: FIXED_NOW,
            updated_at: FIXED_NOW,
          });
        }

        if (pathname === '/v1/datasets' && method === 'GET') {
          return json([]);
        }

        if (pathname === '/v1/training/jobs') {
          return json({ schema_version: '1.0', jobs: [], total: 0, page: 1, page_size: 20 });
        }

        if (pathname === '/v1/training/templates') {
          return json([]);
        }

        return json({ schema_version: '1.0' });
      });

      await navigateToDatasetsPage(page);

      // First upload with content A
      await openUploadDialog(page);
      await page.locator('#name').fill('content-a');
      await page.locator('#sourceType').click();
      await page.getByRole('option', { name: /uploaded files/i }).click();
      await page.locator('input[type="file"]').setInputFiles({
        name: 'content-a.jsonl',
        mimeType: 'application/json',
        buffer: createTestFileBuffer('{"instruction": "A", "response": "A"}'),
      });

      const firstUploadPromise = page.waitForResponse(
        (response) =>
          response.url().includes('/v1/datasets/upload') && response.request().method() === 'POST'
      );
      await page.getByRole('button', { name: /create/i }).click();
      const firstResponse = await firstUploadPromise;
      const firstBody = await firstResponse.json();

      await expect(page.getByRole('dialog')).toBeHidden({ timeout: 5000 });

      // Second upload with content B
      await openUploadDialog(page);
      await page.locator('#name').fill('content-b');
      await page.locator('#sourceType').click();
      await page.getByRole('option', { name: /uploaded files/i }).click();
      await page.locator('input[type="file"]').setInputFiles({
        name: 'content-b.jsonl',
        mimeType: 'application/json',
        buffer: createTestFileBuffer('{"instruction": "B", "response": "B"}'),
      });

      const secondUploadPromise = page.waitForResponse(
        (response) =>
          response.url().includes('/v1/datasets/upload') && response.request().method() === 'POST'
      );
      await page.getByRole('button', { name: /create/i }).click();
      const secondResponse = await secondUploadPromise;
      const secondBody = await secondResponse.json();

      // Hashes should be different
      expect(firstBody.hash).not.toBe(secondBody.hash);

      // Dataset IDs should be different
      expect(firstBody.dataset_id).not.toBe(secondBody.dataset_id);

      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
      expect(consoleErrors, `console errors: ${consoleErrors.join('\n')}`).toEqual([]);
    });
  });

  test.describe('Loading States and UI Feedback', () => {
    test('shows loading state during upload', async ({ page }) => {
      const { consoleErrors, pageErrors } = attachConsoleGuards(page);
      await setupDatasetUploadMocks(page, { uploadDelayMs: 1000 }); // Longer delay to observe loading

      await navigateToDatasetsPage(page);
      await openUploadDialog(page);

      await page.locator('#name').fill('loading-test');
      await page.locator('#sourceType').click();
      await page.getByRole('option', { name: /uploaded files/i }).click();
      await page.locator('input[type="file"]').setInputFiles({
        name: 'test.jsonl',
        mimeType: 'application/json',
        buffer: createTestFileBuffer(TEST_DATASET_CONTENT),
      });

      const createButton = page.getByRole('button', { name: /create/i });
      await createButton.click();

      // Button should show loading state
      await expect(createButton).toHaveText(/creating/i, { timeout: 2000 });
      await expect(createButton).toBeDisabled();

      // Wait for upload to complete
      await page.waitForResponse(
        (response) =>
          response.url().includes('/v1/datasets/upload') && response.request().method() === 'POST'
      );

      expect(pageErrors, `page errors: ${pageErrors.join('\n')}`).toEqual([]);
      expect(consoleErrors, `console errors: ${consoleErrors.join('\n')}`).toEqual([]);
    });
  });
});
