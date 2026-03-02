import { expect, test } from '@playwright/test';
import { gotoAndBootstrap } from './utils';

test('datasets list and manage actions', { tag: ['@ui'] }, async ({ page }) => {
  const datasetId = 'dataset-test-manage';
  const versionId = 'dsv-test-1';
  const now = '2026-03-01T12:00:00Z';

  let deleted = false;
  let trustState = 'needs_approval';
  let validationStatus = 'pending';
  let preprocessPollCount = 0;

  await page.route('**/v1/datasets', async (route) => {
    if (route.request().method() !== 'GET') {
      await route.fallback();
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: '1.0',
        datasets: deleted
          ? []
          : [
            {
              schema_version: '1.0',
              id: datasetId,
              name: 'fixture-dataset',
              display_name: 'Fixture Dataset',
              format: 'jsonl',
              status: 'ready',
              trust_state: trustState,
              validation_status: validationStatus,
              created_at: now,
              updated_at: now,
            },
          ],
        total: deleted ? 0 : 1,
      }),
    });
  });

  await page.route(`**/v1/datasets/${datasetId}`, async (route) => {
    if (route.request().method() !== 'GET') {
      await route.fallback();
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: '1.0',
        id: datasetId,
        dataset_version_id: versionId,
        name: 'fixture-dataset',
        display_name: 'Fixture Dataset',
        format: 'jsonl',
        status: 'ready',
        trust_state: trustState,
        validation_status: validationStatus,
        dataset_hash_b3: 'a'.repeat(64),
        created_at: now,
        updated_at: now,
      }),
    });
  });

  await page.route(`**/v1/datasets/${datasetId}/versions`, async (route) => {
    if (route.request().method() !== 'GET') {
      await route.fallback();
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: '1.0',
        dataset_id: datasetId,
        versions: [
          {
            dataset_version_id: versionId,
            version_number: 1,
            trust_state: trustState,
            created_at: now,
          },
        ],
      }),
    });
  });

  await page.route(`**/v1/datasets/${datasetId}/files`, async (route) => {
    if (route.request().method() !== 'GET') {
      await route.fallback();
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([
        {
          schema_version: '1.0',
          file_id: 'file-1',
          file_name: 'examples.jsonl',
          file_path: 'var/tmp/examples.jsonl',
          size_bytes: 2048,
          hash: 'b'.repeat(64),
          mime_type: 'application/json',
          created_at: now,
        },
      ]),
    });
  });

  await page.route(`**/v1/datasets/${datasetId}/preview*`, async (route) => {
    if (route.request().method() !== 'GET') {
      await route.fallback();
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        dataset_id: datasetId,
        format: 'jsonl',
        total_examples: 2,
        examples: [
          { input: 'hello', target: 'world' },
          { input: 'bye', target: 'moon' },
        ],
      }),
    });
  });

  await page.route(`**/v1/datasets/${datasetId}/validate`, async (route) => {
    if (route.request().method() !== 'POST') {
      await route.fallback();
      return;
    }
    validationStatus = 'valid';
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: '1.0',
        dataset_id: datasetId,
        is_valid: true,
        validation_status: 'valid',
        errors: null,
        validated_at: now,
      }),
    });
  });

  await page.route(`**/v1/datasets/${datasetId}/preprocess`, async (route) => {
    if (route.request().method() !== 'POST') {
      await route.fallback();
      return;
    }
    preprocessPollCount = 0;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        job_id: 'preprocess-job-1',
        dataset_id: datasetId,
        status: 'running',
        message: 'started',
      }),
    });
  });

  await page.route(`**/v1/datasets/${datasetId}/preprocess/status`, async (route) => {
    if (route.request().method() !== 'GET') {
      await route.fallback();
      return;
    }
    preprocessPollCount += 1;
    const running = preprocessPollCount < 2;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        job_id: 'preprocess-job-1',
        dataset_id: datasetId,
        status: running ? 'running' : 'completed',
        pii_scrub: true,
        dedupe: true,
        lines_processed: running ? 10 : 20,
        lines_removed: running ? 1 : 2,
        error_message: null,
        started_at: now,
        completed_at: running ? null : now,
      }),
    });
  });

  await page.route(`**/v1/datasets/${datasetId}/versions/${versionId}/trust-override`, async (route) => {
    if (route.request().method() !== 'POST') {
      await route.fallback();
      return;
    }
    const payload = route.request().postDataJSON() as {
      override_state?: string;
      reason?: string;
    };
    trustState = payload.override_state ?? 'allowed';
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        dataset_id: datasetId,
        dataset_version_id: versionId,
        override_state: payload.override_state ?? 'allowed',
        effective_trust_state: trustState,
        reason: payload.reason ?? null,
      }),
    });
  });

  await page.route(`**/v1/datasets/${datasetId}`, async (route) => {
    if (route.request().method() !== 'DELETE') {
      await route.fallback();
      return;
    }
    deleted = true;
    await route.fulfill({ status: 204, body: '' });
  });

  await gotoAndBootstrap(page, '/datasets', { mode: 'ui-only' });

  await expect(page.getByRole('heading', { name: 'Datasets', level: 1, exact: true })).toBeVisible();
  await expect(page.getByText('Fixture Dataset')).toBeVisible();

  await page.getByRole('button', { name: 'View', exact: true }).click();

  await expect(page).toHaveURL(new RegExp(`/datasets/${datasetId}$`));
  await expect(page.getByRole('heading', { name: 'Dataset Detail', level: 1, exact: true })).toBeVisible();
  await expect(page.getByText('Fixture Dataset')).toBeVisible();
  await expect(page.getByText('examples.jsonl')).toBeVisible();

  await page.getByRole('button', { name: 'Validate Dataset', exact: true }).click();
  await expect(page.getByText(/Validation finished:/)).toBeVisible();

  await page.getByRole('button', { name: 'Preprocess', exact: true }).click();
  await expect(page.getByText(/Preprocessing (started|completed)/)).toBeVisible();
  await expect(page.getByText('Preprocessing completed.')).toBeVisible({ timeout: 15_000 });

  const trustSelects = page.locator('select');
  await trustSelects.nth(1).selectOption('allowed');
  await page.getByPlaceholder('Reason (required)').fill('approve fixture for training');
  await page.getByRole('button', { name: 'Apply Trust Override', exact: true }).click();
  await expect(page.getByText(/Trust override applied/)).toBeVisible();

  const trainLink = page.locator(`a[href*="/training?open_wizard=1&dataset_id=${datasetId}"]`);
  await expect(trainLink).toHaveCount(1);

  await page.getByRole('button', { name: 'Delete Dataset', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Delete dataset', exact: true })).toBeVisible();
  await page
    .getByPlaceholder(`Type ${datasetId} to confirm`)
    .fill(datasetId);
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Delete', exact: true })
    .click();

  await expect(page).toHaveURL(/\/datasets$/);
  await expect(page.getByText('No datasets found')).toBeVisible();
});
