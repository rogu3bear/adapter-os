/// <reference types="node" />
/// <reference types="cypress" />

import { defineConfig } from 'cypress';
import path from 'path';
import { promisify } from 'util';
import { exec as execCb } from 'child_process';

const exec = promisify(execCb);
const workspaceRoot = path.resolve(__dirname, '..');

export default defineConfig({
  e2e: {
    // Port offset strategy: CYPRESS_BASE_URL=http://localhost:${AOS_UI_PORT}
    // Default: 3200, Developer B: 3300 (+100), Developer C: 3400 (+200)
    baseUrl: process.env.CYPRESS_BASE_URL || 'http://localhost:3200',
    specPattern: 'e2e/cypress/e2e/**/*.cy.ts',
    supportFile: 'e2e/cypress/support/index.ts',
    video: true,
    screenshotOnRunFailure: true,
    trashAssetsBeforeRuns: true,
    experimentalRunAllSpecs: false,
    // Increased timeouts for slow API endpoints (model import, inference, etc.)
    defaultCommandTimeout: 30000, // 30 seconds for slow operations
    requestTimeout: 30000, // 30 seconds for API requests
    responseTimeout: 30000, // 30 seconds for response
    pageLoadTimeout: 60000, // 60 seconds for page loads
    env: {
      // Port offset strategy: CYPRESS_API_BASE_URL=http://localhost:${AOS_SERVER_PORT}
      // Default: 8080, Developer B: 8180 (+100), Developer C: 8280 (+200)
      API_BASE_URL: process.env.CYPRESS_API_BASE_URL || 'http://localhost:8080',
      TEST_USER_EMAIL: process.env.CYPRESS_TEST_USER_EMAIL || 'test@example.com',
      TEST_USER_PASSWORD: process.env.CYPRESS_TEST_USER_PASSWORD || 'password',
      TEST_TENANT_ID: process.env.CYPRESS_TEST_TENANT_ID || 'tenant-test',
      TEST_MODEL_ID: process.env.CYPRESS_TEST_MODEL_ID || 'model-qwen-test',
      TEST_ADAPTER_ID: process.env.CYPRESS_TEST_ADAPTER_ID || 'adapter-test',
      TEST_STACK_ID: process.env.CYPRESS_TEST_STACK_ID || 'stack-test',
      TEST_CHAT_SESSION_ID: process.env.CYPRESS_TEST_CHAT_SESSION_ID || 'chat-session-test',
      AUTH_TOKEN: process.env.CYPRESS_AUTH_TOKEN || 'dev-token',
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
        async 'db:seed-fixtures'(options: { skipReset?: boolean; chat?: boolean } = {}) {
          const args = ['run', '-p', 'adapteros-cli', '--', 'db', 'seed-fixtures'];
          if (options.skipReset) {
            args.push('--skip-reset');
          }
          if (options.chat === false) {
            args.push('--no-chat');
          }

          const env = {
            ...process.env,
            AOS_DEV_NO_AUTH: process.env.AOS_DEV_NO_AUTH || '1',
            AOS_DEV_JWT_SECRET: process.env.AOS_DEV_JWT_SECRET || 'test',
            AOS_DETERMINISTIC: process.env.AOS_DETERMINISTIC || '1',
          };

          await exec(`cargo ${args.join(' ')}`, { cwd: workspaceRoot, env });
          return null;
        },
      });
      // TODO: wire up lifecycle orchestration helpers driven by the plan document.
      return config;
    },
  },
});
