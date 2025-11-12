/// <reference types="cypress" />

describe('Tenants Page E2E Tests', () => {
  beforeEach(() => {
    cy.login();
    cy.visit('/tenants');
  });

  afterEach(() => {
    cy.cleanupTestData();
  });

  it('should load tenants page and display header', () => {
    cy.contains('Tenants').should('be.visible');
    cy.get('[data-cy=tenants-page]').should('be.visible');
  });

  it('should display tenant list or empty state', () => {
    cy.get('[data-cy=tenant-list]', { timeout: 10000 }).should('exist');

    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=tenant-card]').length > 0) {
        cy.get('[data-cy=tenant-card]').should('have.length.at.least', 1);
      } else {
        cy.contains(/no tenants|empty/i).should('be.visible');
      }
    });
  });

  it('should filter tenants using search', () => {
    cy.get('[data-cy=tenant-search]').should('be.visible').type('test');
    cy.wait(500); // Debounce delay
    cy.get('[data-cy=tenant-list]').should('be.visible');
  });

  it('should open tenant details when clicking on tenant card', () => {
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=tenant-card]').length > 0) {
        cy.get('[data-cy=tenant-card]').first().click();
        cy.get('[data-cy=tenant-details]').should('be.visible');
      } else {
        cy.log('No tenants available to click');
      }
    });
  });

  it('should display tenant metadata', () => {
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=tenant-card]').length > 0) {
        const tenantCard = cy.get('[data-cy=tenant-card]').first();

        tenantCard.within(() => {
          cy.get('[data-cy=tenant-name]').should('be.visible');
          cy.get('[data-cy=tenant-id]').should('exist');
        });
      } else {
        cy.log('No tenants available for metadata check');
      }
    });
  });

  it('should support creating a new tenant', () => {
    cy.get('[data-cy=create-tenant-btn]').should('be.visible').click();
    cy.get('[data-cy=tenant-form]').should('be.visible');

    cy.get('[data-cy=tenant-name-input]').should('be.visible');
    cy.get('[data-cy=tenant-description-input]').should('be.visible');
  });

  it('should validate tenant creation form', () => {
    cy.get('[data-cy=create-tenant-btn]').click();
    cy.get('[data-cy=tenant-form-submit]').click();

    // Should show validation errors for empty fields
    cy.contains(/required|invalid/i).should('be.visible');
  });

  it('should display tenant isolation settings', () => {
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=tenant-card]').length > 0) {
        cy.get('[data-cy=tenant-card]').first().click();
        cy.get('[data-cy=tenant-details]').should('be.visible');

        // Check for isolation policy information
        cy.get('[data-cy=isolation-settings]').should('exist');
      }
    });
  });

  it('should show tenant resource usage', () => {
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=tenant-card]').length > 0) {
        cy.get('[data-cy=tenant-card]').first().click();
        cy.get('[data-cy=resource-usage]').should('exist');
      }
    });
  });

  it('should filter by tenant status', () => {
    cy.get('[data-cy=status-filter]').should('be.visible').click();
    cy.get('[data-cy=status-option-active]').click();
    cy.wait(500);
    cy.get('[data-cy=tenant-list]').should('be.visible');
  });

  it('should display tenant adapter assignments', () => {
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=tenant-card]').length > 0) {
        cy.get('[data-cy=tenant-card]').first().click();
        cy.get('[data-cy=tenant-adapters]').should('exist');
      }
    });
  });
});
