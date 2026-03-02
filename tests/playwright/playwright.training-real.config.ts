import { defineConfig } from '@playwright/test';
import baseConfig from './playwright.ui.config';

process.env.PW_TRAINING_REAL = '1';
process.env.PW_RUN_ID ??= 'training-real';

export default defineConfig({
  ...baseConfig,
  projects: [{ name: 'chromium', use: { browserName: 'chromium' } }],
  workers: 1,
  grep: /@training-real/,
  testMatch: ['ui/training.create.real.spec.ts'],
});
