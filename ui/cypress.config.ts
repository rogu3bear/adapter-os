/// <reference types="node" />
/// <reference types="cypress" />

import { defineConfig } from 'cypress';

export default defineConfig({
  e2e: {
    baseUrl: process.env.CYPRESS_BASE_URL || 'http://localhost:3200',
    specPattern: 'e2e/cypress/e2e/**/*.cy.ts',
    supportFile: 'e2e/cypress/support/index.ts',
    video: false,
    screenshotOnRunFailure: true,
    // Increased timeouts for slow API endpoints (model import, inference, etc.)
    defaultCommandTimeout: 30000, // 30 seconds for slow operations
    requestTimeout: 30000, // 30 seconds for API requests
    responseTimeout: 30000, // 30 seconds for response
    pageLoadTimeout: 60000, // 60 seconds for page loads
    env: {
      API_BASE_URL: process.env.CYPRESS_API_BASE_URL || 'http://localhost:3300',
      TEST_USER_EMAIL: process.env.CYPRESS_TEST_USER_EMAIL || 'test@example.com',
      TEST_USER_PASSWORD: process.env.CYPRESS_TEST_USER_PASSWORD || 'password',
    },
    setupNodeEvents(on, config) {
      on('task', {
        async 'merge-coverage'(coverage) {
          const fs = require('fs');
          const path = require('path');
          const coveragePath = path.join(__dirname, '.nyc_output');
          if (!fs.existsSync(coveragePath)) {
            fs.mkdirSync(coveragePath, { recursive: true });
          }
          fs.writeFileSync(path.join(coveragePath, 'out.json'), JSON.stringify(coverage));
          return null;
        },
      });
      // TODO: wire up lifecycle orchestration helpers driven by the plan document.
      return config;
    },
  },
});
