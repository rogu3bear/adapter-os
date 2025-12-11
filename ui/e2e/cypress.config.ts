import { defineConfig } from 'cypress';
import path from 'path';

const projectRoot = path.resolve(__dirname);
const supportFile = path.resolve(projectRoot, 'cypress/support/index.ts');

export default defineConfig({
  projectRoot,
  e2e: {
    baseUrl: process.env.CYPRESS_BASE_URL || 'http://localhost:3200',
    specPattern: 'cypress/e2e/**/*.cy.ts',
    supportFile,
    video: false,
    screenshotOnRunFailure: true,
    // Increased timeouts for slow API endpoints (model import, inference, etc.)
    defaultCommandTimeout: 30000, // 30 seconds for slow operations
    requestTimeout: 30000, // 30 seconds for API requests
    responseTimeout: 30000, // 30 seconds for response
    pageLoadTimeout: 60000, // 60 seconds for page loads
    env: {
      API_BASE_URL: process.env.CYPRESS_API_BASE_URL || 'http://localhost:8080',
      TEST_USER_EMAIL: process.env.CYPRESS_TEST_USER_EMAIL || 'test@example.com',
      TEST_USER_PASSWORD: process.env.CYPRESS_TEST_USER_PASSWORD || 'password',
    },
    setupNodeEvents(on, config) {
      // TODO: wire up lifecycle orchestration helpers driven by the plan document.
      return config;
    },
  },
});
