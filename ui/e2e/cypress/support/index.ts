import './commands';
import 'cypress-axe';
import { clearAuthToken } from './api-helpers';
import { clearResourceTracking } from './resource-cleanup';

// Cypress automatically retries commands by default.
// Additional lifecycle helpers (start/stop server, teardown) will be added here.

// Global cleanup: Clear authentication token and resource tracking after each test
// This prevents test pollution and ensures clean state between tests
afterEach(() => {
  // Clear auth token directly (not using cy command to avoid command queue issues)
  clearAuthToken();
  // Clear resource tracking (actual cleanup should be done in test-specific afterEach hooks)
  clearResourceTracking();
});
