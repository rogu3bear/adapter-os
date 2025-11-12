/// <reference types="cypress" />

describe('Training Page E2E Tests', () => {
  beforeEach(() => {
    cy.login();
    cy.visit('/training');
  });

  afterEach(() => {
    cy.cleanupTestData();
  });

  it('should load training page and display header', () => {
    cy.contains(/training|trainer/i).should('be.visible');
    cy.get('[data-cy=training-page]').should('be.visible');
  });

  it('should display training jobs list', () => {
    cy.get('[data-cy=training-jobs-list]', { timeout: 10000 }).should('exist');
  });

  it('should show new training job button', () => {
    cy.get('[data-cy=new-training-job-btn]').should('be.visible');
  });

  it('should open training job creation form', () => {
    cy.get('[data-cy=new-training-job-btn]').click();
    cy.get('[data-cy=training-job-form]').should('be.visible');

    // Check for required form fields
    cy.get('[data-cy=adapter-name-input]').should('be.visible');
    cy.get('[data-cy=template-select]').should('be.visible');
  });

  it('should display training templates', () => {
    cy.get('[data-cy=new-training-job-btn]').click();
    cy.get('[data-cy=template-select]').click();

    cy.get('[data-cy=template-option]').should('have.length.at.least', 1);
  });

  it('should validate training job form', () => {
    cy.get('[data-cy=new-training-job-btn]').click();
    cy.get('[data-cy=training-job-form]').should('be.visible');

    // Try to submit empty form
    cy.get('[data-cy=submit-training-job]').click();

    // Should show validation errors
    cy.contains(/required|invalid/i).should('be.visible');
  });

  it('should configure training hyperparameters', () => {
    cy.get('[data-cy=new-training-job-btn]').click();

    // Expand advanced settings
    cy.get('[data-cy=advanced-settings]').click();

    // Check for hyperparameter fields
    cy.get('[data-cy=rank-input]').should('be.visible');
    cy.get('[data-cy=alpha-input]').should('be.visible');
    cy.get('[data-cy=learning-rate-input]').should('be.visible');
    cy.get('[data-cy=epochs-input]').should('be.visible');
    cy.get('[data-cy=batch-size-input]').should('be.visible');
  });

  it('should display job progress for running jobs', () => {
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=training-job-card]').length > 0) {
        const runningJob = $body.find('[data-cy=job-status="running"]').first();

        if (runningJob.length > 0) {
          cy.wrap(runningJob).within(() => {
            cy.get('[data-cy=progress-bar]').should('be.visible');
            cy.get('[data-cy=progress-pct]').should('be.visible');
          });
        }
      }
    });
  });

  it('should show training metrics', () => {
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=training-job-card]').length > 0) {
        cy.get('[data-cy=training-job-card]').first().click();
        cy.get('[data-cy=training-metrics]').should('exist');
      }
    });
  });

  it('should allow canceling a running job', () => {
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=job-status="running"]').length > 0) {
        cy.get('[data-cy=training-job-card]').first().click();
        cy.get('[data-cy=cancel-job-btn]').should('be.visible');
      }
    });
  });

  it('should display loss and accuracy charts', () => {
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=training-job-card]').length > 0) {
        cy.get('[data-cy=training-job-card]').first().click();
        cy.get('[data-cy=metrics-chart]').should('exist');
      }
    });
  });

  it('should show dataset upload option', () => {
    cy.get('[data-cy=new-training-job-btn]').click();
    cy.get('[data-cy=dataset-upload]').should('be.visible');
  });

  it('should display training job history', () => {
    cy.get('[data-cy=job-history-tab]').click();
    cy.get('[data-cy=completed-jobs-list]').should('exist');
  });

  it('should filter jobs by status', () => {
    cy.get('[data-cy=status-filter]').should('be.visible').click();
    cy.get('[data-cy=status-running]').click();
    cy.wait(500);
    cy.get('[data-cy=training-jobs-list]').should('be.visible');
  });
});
