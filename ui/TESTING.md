# AdapterOS UI Testing Guide

## Overview

This document describes the Cypress integration test suite for the AdapterOS UI. The test suite includes both API tests and UI tests covering authentication, training, adapters, tenants, models, and inference.

## Test Structure

```
ui/e2e/cypress/
├── e2e/
│   ├── api/              # API integration tests
│   │   ├── auth.cy.ts           # Authentication endpoints
│   │   ├── training.cy.ts       # Training job management
│   │   ├── adapters.cy.ts       # Adapter management
│   │   └── tenants.cy.ts        # Tenant management
│   └── ui/               # UI/E2E tests
│       ├── dashboard.cy.ts      # Dashboard page
│       ├── adapters.cy.ts       # Adapters page
│       ├── tenants.cy.ts        # Tenants page
│       ├── models.cy.ts         # Base models page
│       ├── training.cy.ts       # Training page
│       └── inference.cy.ts      # Inference playground
├── support/
│   ├── commands.ts              # Custom Cypress commands
│   ├── api-helpers.ts           # API testing utilities
│   ├── resource-cleanup.ts      # Test resource cleanup
│   └── index.ts                 # Global test configuration
└── cypress.config.ts            # Cypress configuration
```

## Test Coverage

### API Tests

#### Authentication (`api/auth.cy.ts`)
- ✅ Login with valid/invalid credentials
- ✅ Token refresh and rotation
- ✅ Session management
- ✅ Profile management
- ✅ Auth configuration
- ✅ Dev bypass endpoint

#### Training (`api/training.cy.ts`)
- ✅ List training templates
- ✅ Start/cancel training jobs
- ✅ Get training job details and metrics
- ✅ Dataset upload and management
- ✅ Dataset validation and statistics
- ✅ Training events (SSE)

#### Adapters (`api/adapters.cy.ts`)
- ✅ List and filter adapters
- ✅ Get adapter details and manifest
- ✅ Load/unload adapters
- ✅ Register and delete adapters
- ✅ Adapter health checks
- ✅ Routing weights

#### Tenants (`api/tenants.cy.ts`)
- ✅ List and filter tenants
- ✅ Create/update/delete tenants
- ✅ Tenant isolation policies
- ✅ Resource usage and limits
- ✅ Adapter assignments

### UI Tests

#### Dashboard (`ui/dashboard.cy.ts`)
- ✅ Persona slider interaction
- ✅ Widget display
- ✅ Navigation to other pages

#### Adapters Page (`ui/adapters.cy.ts`)
- ✅ Adapter list display
- ✅ Search and filter
- ✅ Adapter details view
- ✅ Upload adapter navigation
- ✅ Status filtering

#### Tenants Page (`ui/tenants.cy.ts`)
- ✅ Tenant list display
- ✅ Search and filter
- ✅ Create tenant form
- ✅ Tenant details and settings
- ✅ Resource usage display

#### Base Models Page (`ui/models.cy.ts`)
- ✅ Model list display
- ✅ Search and filter
- ✅ Model specifications
- ✅ Import model form
- ✅ Compatible adapters

#### Training Page (`ui/training.cy.ts`)
- ✅ Training jobs list
- ✅ New training job form
- ✅ Template selection
- ✅ Hyperparameter configuration
- ✅ Job progress and metrics
- ✅ Dataset upload

#### Inference Page (`ui/inference.cy.ts`)
- ✅ Model and adapter selection
- ✅ Prompt input and validation
- ✅ Inference parameters
- ✅ Run inference
- ✅ Results and statistics
- ✅ Streaming support

## Running Tests

### Prerequisites

1. **Install Cypress** (if not already installed):
   ```bash
   pnpm install
   npx cypress install
   ```

2. **Configure environment variables** (optional):
   ```bash
   export CYPRESS_BASE_URL=http://localhost:3200
   export CYPRESS_API_BASE_URL=http://localhost:8080
   export CYPRESS_TEST_USER_EMAIL=test@example.com
   export CYPRESS_TEST_USER_PASSWORD=password
   ```

### Run All Tests

#### Headless Mode (CI)
```bash
# Start server and run all tests
pnpm run test:e2e

# Or run tests manually (requires server running separately)
pnpm run cypress:run
```

#### Interactive Mode (Development)
```bash
# Open Cypress Test Runner
pnpm run cypress:open

# Then select tests to run interactively
```

### Run Specific Tests

#### API Tests Only
```bash
npx cypress run --spec "e2e/cypress/e2e/api/**/*.cy.ts"
```

#### UI Tests Only
```bash
npx cypress run --spec "e2e/cypress/e2e/ui/**/*.cy.ts"
```

#### Single Test File
```bash
npx cypress run --spec "e2e/cypress/e2e/api/auth.cy.ts"
```

### Run with Coverage
```bash
pnpm run test:e2e:coverage
pnpm run coverage:report
```

## Custom Commands

### Authentication
```typescript
// Login and cache token
cy.login()

// Make authenticated API request
cy.apiRequest({
  method: 'GET',
  url: '/v1/adapters',
})

// Clear authentication
cy.clearAuth()
```

### Resource Cleanup
```typescript
// Track resource for cleanup
cy.trackResource('adapter', adapterId, '/v1/adapters/${adapterId}')

// Clean up all tracked resources
cy.cleanupTestData()
```

## Test Utilities

### API Helpers (`support/api-helpers.ts`)
- `computeRequestId()` - Deterministic request ID generation
- `validateErrorResponse()` - Error response validation
- `validateLoginResponse()` - Login response validation
- `getApiBaseUrl()` - Get API base URL from env
- `getTestCredentials()` - Get test user credentials

### Resource Cleanup (`support/resource-cleanup.ts`)
- Automatic cleanup of created resources after each test
- Tracks: adapters, tenants, training jobs, datasets
- Cleanup runs in reverse order of creation

## Configuration

### Cypress Config (`cypress.config.ts`)
```typescript
{
  baseUrl: 'http://localhost:3200',     // UI server
  API_BASE_URL: 'http://localhost:8080', // API server

  // Increased timeouts for slow operations
  defaultCommandTimeout: 30000,
  requestTimeout: 30000,
  responseTimeout: 30000,
  pageLoadTimeout: 60000,
}
```

### Environment Variables
- `CYPRESS_BASE_URL` - UI server URL (default: http://localhost:3200)
- `CYPRESS_API_BASE_URL` - API server URL (default: http://localhost:8080)
- `CYPRESS_TEST_USER_EMAIL` - Test user email
- `CYPRESS_TEST_USER_PASSWORD` - Test user password
- `CYPRESS_COVERAGE` - Enable coverage collection (set to 1)

## Writing New Tests

### Test Structure
```typescript
describe('Feature Name', () => {
  beforeEach(() => {
    cy.login();
    cy.visit('/page-path');
  });

  afterEach(() => {
    cy.cleanupTestData();
  });

  it('should perform action', () => {
    // Test implementation
  });
});
```

### Best Practices

1. **Use data-cy attributes** for element selection:
   ```typescript
   cy.get('[data-cy=submit-button]').click()
   ```

2. **Track created resources** for cleanup:
   ```typescript
   cy.trackResource('type', id, '/v1/resource/${id}')
   ```

3. **Handle optional elements** gracefully:
   ```typescript
   cy.get('body').then(($body) => {
     if ($body.find('[data-cy=element]').length > 0) {
       // Element exists
     } else {
       cy.log('Element not found');
     }
   });
   ```

4. **Use failOnStatusCode: false** for error testing:
   ```typescript
   cy.apiRequest({
     method: 'GET',
     url: '/v1/invalid',
     failOnStatusCode: false,
   }).then((response) => {
     expect(response.status).to.eq(404);
   });
   ```

## Troubleshooting

### Tests failing with "Server not running"
Start the dev server before running tests:
```bash
pnpm run dev
# In another terminal:
pnpm run cypress:run
```

Or use the combined command:
```bash
pnpm run test:e2e
```

### Authentication errors
1. Check API server is running
2. Verify test credentials in environment variables
3. Check dev bypass endpoint is enabled (if in dev mode)

### Timeout errors
Increase timeouts in `cypress.config.ts`:
```typescript
defaultCommandTimeout: 60000,
requestTimeout: 60000,
```

### Resource cleanup issues
Resources are tracked and cleaned up automatically. To manually clean:
```typescript
cy.cleanupTestData()
```

## CI/CD Integration

### GitHub Actions Example
```yaml
- name: Install dependencies
  run: pnpm install

- name: Install Cypress
  run: npx cypress install

- name: Run E2E tests
  run: pnpm run test:e2e:coverage

- name: Upload coverage
  uses: codecov/codecov-action@v3
  with:
    files: ./coverage/lcov.info
```

## Test Metrics

Current test count:
- **API Tests**: ~60 tests across 4 files
- **UI Tests**: ~50 tests across 6 files
- **Total**: ~110 integration tests

Coverage areas:
- Authentication: ✅ Complete
- Training: ✅ Complete
- Adapters: ✅ Complete
- Tenants: ✅ Complete
- Models: ✅ Basic
- Inference: ✅ Basic
- Dashboard: ✅ Basic

## Future Improvements

- [ ] Add visual regression tests
- [ ] Add performance benchmarks
- [ ] Increase UI test coverage for remaining pages
- [ ] Add accessibility tests (a11y)
- [ ] Add network stubbing for offline testing
- [ ] Add component tests
