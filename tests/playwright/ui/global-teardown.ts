import { request, type FullConfig } from '@playwright/test';

const backendBaseUrl = 'http://localhost:8080';

export default async function globalTeardown(_config: FullConfig) {
  try {
    const api = await request.newContext({ baseURL: backendBaseUrl });
    await api.post('/testkit/reset');
    await api.dispose();
  } catch {
    // Best-effort cleanup only.
  }
}
