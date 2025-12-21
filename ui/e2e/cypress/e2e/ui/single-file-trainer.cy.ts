/// <reference types="cypress" />
import '../../support/commands';

/**
 * Verifies the single-file trainer flow now uses dataset_id + post_actions via:
 * upload document -> process -> create dataset -> start training -> show adapter/stack outputs.
 *
 * All network calls are stubbed to avoid long-running training or embeddings work.
 */
const ensureLoginCommand = () => {
  const commands: any = (Cypress.Commands as any)._commands || {};
  if (!commands.login) {
    Cypress.Commands.add('login', () =>
      cy.window().then((win) => {
        win.localStorage.setItem('authToken', 'dev-token');
        return 'dev-token';
      })
    );
  }
};

describe('Single File Trainer dataset-first flow', () => {
  beforeEach(() => {
    ensureLoginCommand();

    // Stub auth endpoints to bypass real backend auth
    cy.intercept('POST', '**/v1/auth/login', {
      statusCode: 200,
      body: {
        token: 'dev-token',
        user_id: 'user-1',
        tenant_id: 'default',
        role: 'admin',
        expires_in: 3600,
        tenants: [{ id: 'default', name: 'Default Tenant', role: 'admin' }],
      },
    }).as('login');

    cy.intercept('GET', '**/v1/auth/me', {
      statusCode: 200,
      body: {
        user_id: 'user-1',
        email: 'dev@local',
        display_name: 'Dev User',
        role: 'admin',
        tenant_id: 'default',
        permissions: ['training:start', 'dataset:upload'],
        admin_tenants: ['*'],
        mfa_enabled: false,
      },
    }).as('authMe');

    cy.intercept('POST', '**/v1/auth/refresh', {
      statusCode: 200,
      body: {
        token: 'dev-token',
        user_id: 'user-1',
        tenant_id: 'default',
        role: 'admin',
        expires_in: 3600,
      },
    });

    cy.login();

    // Seed tenant context so UI skips tenant picker
    cy.window().then((win) => {
      win.localStorage.setItem('selectedTenant', 'default');
      win.sessionStorage.setItem('aos-tenant-bootstrap', JSON.stringify([{ id: 'default', name: 'Default Tenant' }]));
    });
  });

  it('uploads a file, builds a dataset, and starts training with post_actions', () => {
    // Stub upload → process → dataset → training → job poll
    cy.intercept('POST', '**/v1/documents/upload', (req) => {
      req.reply({
        statusCode: 200,
        body: {
          schema_version: '1.0',
          document_id: 'doc_123',
          name: 'trainer-sample.md',
          hash_b3: 'hash-doc',
          size_bytes: 128,
          mime_type: 'text/markdown',
          storage_path: 'var/test-documents/doc_123',
          status: 'processing',
          chunk_count: null,
          tenant_id: 'default',
          created_at: new Date().toISOString(),
          updated_at: null,
          deduplicated: false,
        },
      });
    }).as('uploadDoc');

    cy.intercept('POST', '**/v1/documents/**/process', (req) => {
      req.reply({
        statusCode: 200,
        body: {
          schema_version: '1.0',
          document_id: 'doc_123',
          status: 'indexed',
          chunk_count: 3,
          indexed_at: new Date().toISOString(),
        },
      });
    }).as('processDoc');

    cy.intercept('POST', '**/v1/datasets/from-documents', (req) => {
      expect(req.body.document_ids).to.deep.equal(['doc_123']);
      req.reply({
        statusCode: 200,
        body: {
          schema_version: '1.0',
          dataset_id: 'ds_123',
          name: req.body.name ?? 'Training from doc',
          description: req.body.description,
          file_count: 1,
          total_size_bytes: 512,
          format: 'jsonl',
          hash: 'hash-ds',
          storage_path: 'var/test-datasets/ds_123',
          validation_status: 'valid',
          created_by: 'tester',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        },
      });
    }).as('createDataset');

    cy.intercept('POST', '**/v1/training/start', (req) => {
      expect(req.body.dataset_id).to.eq('ds_123');
      expect(req.body.post_actions).to.deep.include({ create_stack: true, activate_stack: true });
      req.reply({
        statusCode: 200,
        body: {
          id: 'job_123',
          adapter_name: req.body.adapter_name,
          dataset_id: req.body.dataset_id,
          adapter_id: 'adapter_123',
          stack_id: 'stack_456',
          aos_path: 'var/test-adapters/adapter_123.aos',
          status: 'running',
          progress_pct: 10,
          created_at: new Date().toISOString(),
        },
      });
    }).as('startTraining');

    cy.intercept('GET', '**/v1/training/jobs/job_123', {
      statusCode: 200,
      body: {
        id: 'job_123',
        adapter_name: 'default/training/trainer-sample/r001',
        dataset_id: 'ds_123',
        adapter_id: 'adapter_123',
        stack_id: 'stack_456',
        aos_path: 'var/test-adapters/adapter_123.aos',
        status: 'completed',
        progress_pct: 100,
        current_epoch: 3,
        total_epochs: 3,
        created_at: new Date().toISOString(),
        completed_at: new Date().toISOString(),
      },
    }).as('getTrainingJob');

    cy.visit('/trainer', {
      onBeforeLoad(win) {
        win.localStorage.setItem('selectedTenant', 'default');
        win.sessionStorage.setItem('aos-tenant-bootstrap', JSON.stringify([{ id: 'default', name: 'Default Tenant' }]));
        win.sessionStorage.setItem('aos-auth-active', 'true');
      },
    });
    cy.get('[data-cy=trainer-root]', { timeout: 30000 }).should('be.visible');
    cy.get('[data-cy=trainer-title]').should('contain.text', 'Single-File Adapter Trainer');

    // Upload file
    cy.get('[data-cy=trainer-file-input]').selectFile('cypress/fixtures/api/trainer-sample.md', {
      force: true,
    });
    cy.get('[data-cy=trainer-continue-btn]').click();

    // Start training (will trigger upload/process/dataset/start/poll)
    cy.get('[data-cy=trainer-start-btn]').click();

    cy.wait('@uploadDoc');
    cy.wait('@processDoc');
    cy.wait('@createDataset');
    cy.wait('@startTraining');
    cy.wait('@getTrainingJob');

    // Completed view should reflect adapter + stack + artifact links
    cy.contains(/Training Complete/i).should('be.visible');
    cy.contains('adapter_123').should('be.visible');
    cy.contains('stack_456').should('be.visible');
    cy.contains('Download .aos').should('be.visible');
    cy.contains('View Stack').should('be.visible');
  });
});
