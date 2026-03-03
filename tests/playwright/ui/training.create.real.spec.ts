import { expect, test, type Page } from '@playwright/test';
import { gotoAndBootstrap, seeded, waitForAppReady } from './utils';

type WorkerResponse = {
  capabilities_detail?: {
    gpu_backward?: boolean;
  } | null;
};

type DatasetResponse = {
  id?: string;
  dataset_id?: string;
};

type TrainingJobParamsRequest = {
  rank?: number;
  alpha?: number;
  epochs?: number;
  learning_rate?: number;
  batch_size?: number;
  targets?: string[];
  training_contract_version?: string;
};

type CreateTrainingJobRequest = {
  base_model_id?: string;
  dataset_id?: string;
  dataset_version_id?: string | null;
  adapter_name?: string;
  params?: TrainingJobParamsRequest;
};

function isTrainingJobsUrl(url: string): boolean {
  const pathname = new URL(url).pathname;
  return pathname === '/v1/training/jobs';
}

function readDatasetId(payload: DatasetResponse): string | undefined {
  return payload.dataset_id ?? payload.id;
}

async function csrfHeaders(page: Page): Promise<Record<string, string>> {
  const cookies = await page.context().cookies();
  const csrfToken = cookies.find((cookie) => cookie.name === 'csrf_token')?.value;
  return csrfToken ? { 'X-CSRF-Token': csrfToken } : {};
}

async function assertGpuTrainingWorker(page: Page): Promise<void> {
  const workersResponse = await page.request.get('/v1/workers?include_inactive=true', {
    timeout: 30_000,
  });
  if (!workersResponse.ok()) {
    const body = await workersResponse.text().catch(() => '');
    throw new Error(
      `Real-training preflight failed to list workers: ${workersResponse.status()} ${body}`
    );
  }
  const workers = (await workersResponse.json()) as WorkerResponse[];
  const hasGpuBackward = workers.some(
    (worker) => worker.capabilities_detail?.gpu_backward === true
  );
  if (!hasGpuBackward) {
    throw new Error(
      'Real-training lane requires at least one worker with gpu_backward capability.'
    );
  }
}

async function ensureIndexedFixtureDocument(page: Page): Promise<void> {
  const response = await page.request.post('/testkit/create_document_fixture', {
    data: {
      document_id: seeded.documentId,
      status: 'indexed',
      name: 'Fixture Document',
    },
    timeout: 30_000,
  });
  if (!response.ok()) {
    const body = await response.text().catch(() => '');
    throw new Error(
      `Real-training preflight failed to ensure indexed document fixture: ${response.status()} ${body}`
    );
  }
}

async function ensureDatasetFixture(page: Page): Promise<void> {
  const response = await page.request.post('/testkit/create_dataset_fixture', {
    data: {
      dataset_id: seeded.datasetId,
      name: 'Test Dataset',
    },
    timeout: 30_000,
  });
  if (!response.ok()) {
    const body = await response.text().catch(() => '');
    throw new Error(
      `Real-training preflight failed to ensure dataset fixture: ${response.status()} ${body}`
    );
  }
}

async function ensureDatasetVersionAllowed(page: Page, datasetId: string): Promise<void> {
  const versionsResponse = await page.request.get(`/v1/datasets/${datasetId}/versions`, {
    timeout: 30_000,
  });
  if (!versionsResponse.ok()) {
    const body = await versionsResponse.text().catch(() => '');
    throw new Error(
      `Failed to list dataset versions for ${datasetId}: ${versionsResponse.status()} ${body}`
    );
  }

  const payload = (await versionsResponse.json()) as {
    versions?: Array<{ dataset_version_id?: string }>;
  };
  const datasetVersionId = payload.versions?.[0]?.dataset_version_id;
  if (!datasetVersionId) {
    throw new Error(`Dataset ${datasetId} does not have a dataset_version_id`);
  }

  const overrideResponse = await page.request.post(
    `/v1/datasets/${datasetId}/versions/${datasetVersionId}/trust-override`,
    {
      data: {
        override_state: 'allowed',
        reason: 'real-training playwright fixture',
      },
      headers: await csrfHeaders(page),
      timeout: 30_000,
    }
  );
  if (!overrideResponse.ok()) {
    const body = await overrideResponse.text().catch(() => '');
    throw new Error(
      `Failed to apply trust override for dataset ${datasetId}: ${overrideResponse.status()} ${body}`
    );
  }
}

async function openWizard(page: Page): Promise<void> {
  await gotoAndBootstrap(page, '/training', { mode: 'ui-only' });
  await waitForAppReady(page);
  await page.getByRole('button', { name: 'Create Adapter', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Create Adapter', exact: true })).toBeVisible();
}

async function advanceToConfirm(page: Page, adapterName: string): Promise<void> {
  await expect(page.getByRole('button', { name: 'Next', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await page.getByLabel('Adapter name').fill(adapterName);
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Start training', exact: true })).toBeVisible();
}

test('dataset path starts training via typed training-jobs endpoint', { tag: ['@training-real'] }, async ({
  page,
}) => {
  await gotoAndBootstrap(page, '/training', { mode: 'ui-only' });
  await waitForAppReady(page);
  await assertGpuTrainingWorker(page);
  await ensureIndexedFixtureDocument(page);
  await ensureDatasetFixture(page);
  await ensureDatasetVersionAllowed(page, seeded.datasetId);
  const seededDatasetId = seeded.datasetId;

  await openWizard(page);

  const sourceSelects = page.locator('.wizard-step-content select');
  await expect(sourceSelects.first().locator(`option[value="${seededDatasetId}"]`)).toHaveCount(
    1,
    { timeout: 30_000 }
  );
  await sourceSelects.first().selectOption(seededDatasetId);
  await page.getByRole('button', { name: 'Use selected dataset', exact: true }).click();

  const adapterName = `real-dataset-${Date.now().toString().slice(-6)}`;
  await advanceToConfirm(page, adapterName);

  const trainingRequest = page.waitForRequest(
    (request) => request.method() === 'POST' && isTrainingJobsUrl(request.url()),
    { timeout: 30_000 }
  );
  const trainingResponse = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' && isTrainingJobsUrl(response.url()),
    { timeout: 30_000 }
  );

  await page.getByRole('button', { name: 'Start training', exact: true }).click();

  const request = await trainingRequest;
  const response = await trainingResponse;
  expect(new URL(request.url()).pathname).toBe('/v1/training/jobs');
  const requestPayload = request.postDataJSON() as CreateTrainingJobRequest;
  expect(requestPayload.dataset_id).toBe(seededDatasetId);
  expect(requestPayload.adapter_name).toBe(adapterName);
  expect(requestPayload.base_model_id).toBeTruthy();
  expect(requestPayload.params).toBeTruthy();
  expect(typeof requestPayload.params?.rank).toBe('number');
  expect(typeof requestPayload.params?.alpha).toBe('number');
  expect(typeof requestPayload.params?.epochs).toBe('number');
  expect(typeof requestPayload.params?.learning_rate).toBe('number');
  expect(typeof requestPayload.params?.batch_size).toBe('number');
  expect(Array.isArray(requestPayload.params?.targets)).toBeTruthy();
  expect(requestPayload.params?.training_contract_version).toBeTruthy();
  if (!response.ok()) {
    const body = await response.text().catch(() => '');
    throw new Error(
      `Training jobs request failed for dataset path: ${response.status()} ${body}`
    );
  }

  const payload = (await response.json()) as { id?: string };
  expect(payload.id).toBeTruthy();
});

test('document path converts then starts training via typed training-jobs endpoint', { tag: ['@training-real'] }, async ({
  page,
}) => {
  await gotoAndBootstrap(page, '/training', { mode: 'ui-only' });
  await waitForAppReady(page);
  await assertGpuTrainingWorker(page);
  await ensureIndexedFixtureDocument(page);

  await openWizard(page);

  const sourceSelects = page.locator('.wizard-step-content select');
  await sourceSelects.nth(1).selectOption(seeded.documentId);

  const conversionRequest = page.waitForRequest(
    (request) =>
      request.method() === 'POST' && new URL(request.url()).pathname === '/v1/datasets/from-documents',
    { timeout: 30_000 }
  );
  const conversionResponse = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' &&
      new URL(response.url()).pathname === '/v1/datasets/from-documents',
    { timeout: 30_000 }
  );

  await page.getByRole('button', { name: 'Convert selected document', exact: true }).click();

  await conversionRequest;
  const datasetResponse = await conversionResponse;
  expect(datasetResponse.ok()).toBeTruthy();
  const convertedDataset = (await datasetResponse.json()) as DatasetResponse;
  const convertedDatasetId = readDatasetId(convertedDataset);
  if (!convertedDatasetId) {
    throw new Error('Document conversion response did not include dataset_id');
  }
  await ensureDatasetVersionAllowed(page, convertedDatasetId);

  const adapterName = `real-document-${Date.now().toString().slice(-6)}`;
  await advanceToConfirm(page, adapterName);

  const trainingRequest = page.waitForRequest(
    (request) => request.method() === 'POST' && isTrainingJobsUrl(request.url()),
    { timeout: 30_000 }
  );
  const trainingResponse = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' && isTrainingJobsUrl(response.url()),
    { timeout: 30_000 }
  );

  await page.getByRole('button', { name: 'Start training', exact: true }).click();

  const request = await trainingRequest;
  const response = await trainingResponse;
  expect(new URL(request.url()).pathname).toBe('/v1/training/jobs');
  const requestPayload = request.postDataJSON() as CreateTrainingJobRequest;
  expect(requestPayload.dataset_id).toBe(convertedDatasetId);
  expect(requestPayload.adapter_name).toBe(adapterName);
  expect(requestPayload.base_model_id).toBeTruthy();
  expect(requestPayload.params).toBeTruthy();
  expect(typeof requestPayload.params?.rank).toBe('number');
  expect(typeof requestPayload.params?.alpha).toBe('number');
  expect(typeof requestPayload.params?.epochs).toBe('number');
  expect(typeof requestPayload.params?.learning_rate).toBe('number');
  expect(typeof requestPayload.params?.batch_size).toBe('number');
  expect(Array.isArray(requestPayload.params?.targets)).toBeTruthy();
  expect(requestPayload.params?.training_contract_version).toBeTruthy();
  if (!response.ok()) {
    const body = await response.text().catch(() => '');
    throw new Error(
      `Training jobs request failed for document path: ${response.status()} ${body}`
    );
  }
  const payload = (await response.json()) as { id?: string };
  expect(payload.id).toBeTruthy();
});
