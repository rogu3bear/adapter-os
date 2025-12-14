import { test, expect, Page } from '@playwright/test';

const MOCK_PROMPT = 'Test prompt for adapter + receipt rendering';
const MOCK_RECEIPT_DIGEST = 'b3-mock-receipt-digest-1234567890abcdef';

async function setupInferenceMocks(page: Page, options?: { backendsDelayMs?: number }) {
  const now = new Date().toISOString();
  const backendsDelayMs = options?.backendsDelayMs ?? 0;
  const inferenceResponse = {
    schema_version: '1.0',
    id: 'resp-1',
    text: 'Mocked inference output text',
    tokens_generated: 8,
    token_count: 8,
    latency_ms: 42,
    adapters_used: ['adapter-hot'],
    finish_reason: 'stop' as const,
    backend: 'coreml' as const,
    backend_used: 'coreml',
    run_receipt: {
      trace_id: 'trace-abc123',
      run_head_hash: 'head-hash-mock',
      output_digest: 'output-digest-mock',
      receipt_digest: MOCK_RECEIPT_DIGEST,
    },
    trace: {
      latency_ms: 42,
      adapters_used: ['adapter-hot'],
      router_decisions: [],
      evidence_spans: [],
    },
  };

  await page.route('**/v1/**', async (route) => {
    const url = new URL(route.request().url());
    const { pathname } = url;
    const method = route.request().method();

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
        user_id: 'user-1',
        email: 'dev@local',
        role: 'admin',
        created_at: now,
        display_name: 'Dev User',
        tenant_id: 'tenant-1',
        permissions: ['inference:execute'],
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
        token: 'mock-token',
        user_id: 'user-1',
        tenant_id: 'tenant-1',
        role: 'admin',
        expires_in: 3600,
        tenants: [{ id: 'tenant-1', name: 'System', role: 'admin' }],
      });
    }

    if (pathname === '/v1/models') {
      return json({
        models: [
          {
            id: 'model-1',
            name: 'Demo Model',
            hash_b3: 'hash-model',
            config_hash_b3: 'hash-config',
            tokenizer_hash_b3: 'hash-tokenizer',
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

    if (pathname === '/v1/models/model-1/validate') {
      return json({
        model_id: 'model-1',
        status: 'ready',
        valid: true,
        can_load: true,
        issues: [],
      });
    }

    if (pathname === '/v1/models/model-1/status') {
      return json({
        schema_version: '1.0',
        model_id: 'model-1',
        status: 'ready',
        is_loaded: true,
      });
    }

    if (pathname === '/v1/backends') {
      if (backendsDelayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, backendsDelayMs));
      }
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
      if (backendsDelayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, backendsDelayMs));
      }
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

    if (pathname === '/v1/adapters') {
      return json([
        {
          id: 'adapter-hot',
          name: 'Hot Adapter',
          adapter_id: 'adapter-hot',
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
          id: 'stack-1',
          name: 'Demo Stack',
          adapter_ids: ['adapter-hot'],
          description: 'Demo stack',
          created_at: now,
          updated_at: now,
        },
      ]);
    }

    if (pathname === '/v1/tenants/tenant-1/default-stack') {
      return json({ schema_version: '1.0', stack_id: null });
    }

    if (pathname === '/v1/infer' && method === 'POST') {
      return json(inferenceResponse);
    }

    // Health and telemetry calls that may appear during startup
    if (pathname === '/v1/readyz') {
      return json({
        ready: true,
        checks: {
          db: { ok: true },
          worker: { ok: true },
          models_seeded: { ok: true },
        },
      });
    }

    return json({});
  });
}

test.describe('Inference Playground smoke', () => {
  test('renders output, adapters, and receipt digest', async ({ page }) => {
    await setupInferenceMocks(page, { backendsDelayMs: 800 });

    await page.goto('/inference');

    const backendLoadingMarker = page.getByLabel('Loading backend status');
    await expect(backendLoadingMarker).toBeVisible();
    await expect(backendLoadingMarker).toBeHidden();

    await page.locator('[data-cy="prompt-input"]').fill(MOCK_PROMPT);

    const firstInference = page.waitForResponse(
      (response) => response.url().includes('/v1/infer') && response.request().method() === 'POST'
    );
    await page.locator('[data-cy="run-inference-btn"]').click();
    await firstInference;

    await expect(page.locator('[data-cy="inference-result"]')).toContainText('Mocked inference output text');
    await expect(page.locator('[data-cy="adapter-list"]')).toContainText('adapter-hot');
    await expect(page.locator('[data-cy="receipt-digest"]')).toContainText(MOCK_RECEIPT_DIGEST);

    const firstDigest = (await page.locator('[data-cy="receipt-digest"]').textContent())?.trim();

    const secondInference = page.waitForResponse(
      (response) => response.url().includes('/v1/infer') && response.request().method() === 'POST'
    );
    await page.locator('[data-cy="run-inference-btn"]').click();
    await secondInference;

    const secondDigest = (await page.locator('[data-cy="receipt-digest"]').textContent())?.trim();
    expect(secondDigest).toBe(firstDigest);
  });
});
