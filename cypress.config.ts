import { defineConfig } from 'cypress';

const baseUrl =
  process.env.CYPRESS_baseUrl ||
  process.env.CYPRESS_BASE_URL ||
  'http://127.0.0.1:3200';
const apiUrl =
  process.env.CYPRESS_API_URL ||
  process.env.CYPRESS_apiUrl ||
  process.env.CYPRESS_API_BASE_URL ||
  'http://127.0.0.1:8080';
const e2eUser = process.env.CYPRESS_E2E_USER || process.env.CYPRESS_TEST_USER_EMAIL || 'dev@local';
const e2ePass =
  process.env.CYPRESS_E2E_PASS ||
  process.env.CYPRESS_TEST_USER_PASSWORD ||
  'dev123';

export default defineConfig({
  e2e: {
    baseUrl,
    specPattern: [
      'cypress/e2e/**/*.cy.{ts,tsx}',
      'ui/e2e/cypress/e2e/**/*.cy.{ts,tsx}',
    ],
    supportFile: 'cypress/support/e2e.ts',
    video: true,
    screenshotOnRunFailure: true,
    screenshotsFolder: 'cypress/artifacts/screenshots',
    videosFolder: 'cypress/artifacts/videos',
    downloadsFolder: 'cypress/artifacts/downloads',
    trashAssetsBeforeRuns: true,
    retries: {
      runMode: 2,
      openMode: 0,
    },
    viewportWidth: 1280,
    viewportHeight: 720,
    defaultCommandTimeout: 30000,
    requestTimeout: 30000,
    responseTimeout: 30000,
    pageLoadTimeout: 60000,
    env: {
      API_URL: apiUrl,
      API_BASE_URL: apiUrl,
      E2E_USER: e2eUser,
      E2E_PASS: e2ePass,
      DISABLE_ANIMATIONS: process.env.CYPRESS_DISABLE_ANIMATIONS ?? '1',
    },
    setupNodeEvents(on, config) {
      return config;
    },
  },
});
