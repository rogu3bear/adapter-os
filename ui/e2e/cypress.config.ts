import { defineConfig } from 'cypress';
import path from 'path';
import { execFileSync } from 'child_process';

const projectRoot = path.resolve(__dirname);
const supportFile = path.resolve(projectRoot, 'cypress/support/index.ts');
const repoRoot = path.resolve(projectRoot, '..', '..');
const resetScript = path.resolve(repoRoot, 'scripts/e2e/reset_db.sh');
const seedMinimalScript = path.resolve(repoRoot, 'scripts/e2e/seed_minimal.sh');

export default defineConfig({
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
      // Hard reset before every spec to keep suites independent (Pattern A)
      on('before:spec', () => {
        execFileSync('bash', [resetScript], { cwd: repoRoot, stdio: 'inherit' });
      });

      on('task', {
        'db:reset'() {
          execFileSync('bash', [resetScript], { cwd: repoRoot, stdio: 'inherit' });
          return null;
        },
        'db:seed-fixtures'(options?: { skipReset?: boolean; chat?: boolean }) {
          const useMinimal = options?.skipReset === true;
          const script = useMinimal ? seedMinimalScript : resetScript;
          execFileSync('bash', [script], { cwd: repoRoot, stdio: 'inherit' });
          return null;
        },
        'db:seed-minimal'() {
          execFileSync('bash', [seedMinimalScript], { cwd: repoRoot, stdio: 'inherit' });
          return null;
        },
      });
      return config;
    },
  },
});
