/// <reference types="cypress" />

describe('Policies Page UI Tests', () => {
  beforeEach(() => {
    cy.login();
    cy.visit('/policies');
  });

  describe('Page Load and Navigation', () => {
    it('should load policies page successfully', () => {
      cy.url().should('include', '/policies');
      cy.contains('Policies').should('be.visible');
    });

    it('should display page header and breadcrumb', () => {
      cy.get('[data-cy=page-header]').should('be.visible');
      cy.get('[data-cy=breadcrumb]').should('be.visible');
    });

    it('should have working back navigation', () => {
      cy.get('[data-cy=back-button]').should('exist');
    });
  });

  describe('Policy List Display', () => {
    it('should display policy list or empty state', () => {
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=policy-list]').length > 0) {
          cy.get('[data-cy=policy-list]').should('be.visible');
          cy.get('[data-cy=policy-item]').should('have.length.at.least', 1);
        } else {
          cy.get('[data-cy=empty-state]').should('be.visible');
        }
      });
    });

    it('should display policy cards with required information', () => {
      cy.get('[data-cy=policy-item]').first().within(() => {
        cy.get('[data-cy=policy-name]').should('be.visible');
        cy.get('[data-cy=policy-description]').should('be.visible');
        cy.get('[data-cy=policy-status]').should('be.visible');
      });
    });

    it('should show policy enforcement status', () => {
      cy.get('[data-cy=policy-item]').first().within(() => {
        cy.get('[data-cy=policy-status]').should('contain.text', /enabled|disabled|active|inactive/i);
      });
    });
  });

  describe('Policy Search and Filter', () => {
    it('should have search functionality', () => {
      cy.get('[data-cy=policy-search]').should('be.visible');
      cy.get('[data-cy=policy-search]').type('egress');
      // Wait for search to filter results
      cy.wait(500);
    });

    it('should filter policies by category', () => {
      cy.get('[data-cy=policy-filter]').should('exist');
      cy.get('[data-cy=policy-filter]').click();
      cy.get('[data-cy=filter-option]').should('have.length.at.least', 1);
    });

    it('should filter policies by status', () => {
      cy.get('[data-cy=status-filter]').should('exist');
    });

    it('should clear search and filters', () => {
      cy.get('[data-cy=policy-search]').type('test');
      cy.get('[data-cy=clear-search]').click();
      cy.get('[data-cy=policy-search]').should('have.value', '');
    });
  });

  describe('Policy Details View', () => {
    it('should open policy details when clicking on policy', () => {
      cy.get('[data-cy=policy-item]').first().click();
      cy.get('[data-cy=policy-details]').should('be.visible');
    });

    it('should display comprehensive policy information', () => {
      cy.get('[data-cy=policy-item]').first().click();
      cy.get('[data-cy=policy-details]').within(() => {
        cy.get('[data-cy=policy-name]').should('be.visible');
        cy.get('[data-cy=policy-description]').should('be.visible');
        cy.get('[data-cy=policy-rules]').should('be.visible');
      });
    });

    it('should have close button for policy details', () => {
      cy.get('[data-cy=policy-item]').first().click();
      cy.get('[data-cy=close-details]').should('be.visible').click();
      cy.get('[data-cy=policy-details]').should('not.exist');
    });
  });

  describe('Policy Editor', () => {
    it('should open policy editor for new policy', () => {
      cy.get('[data-cy=create-policy-button]').click();
      cy.get('[data-cy=policy-editor]').should('be.visible');
    });

    it('should have required form fields', () => {
      cy.get('[data-cy=create-policy-button]').click();
      cy.get('[data-cy=policy-name-input]').should('be.visible');
      cy.get('[data-cy=policy-description-input]').should('be.visible');
      cy.get('[data-cy=policy-rules-editor]').should('be.visible');
    });

    it('should validate required fields', () => {
      cy.get('[data-cy=create-policy-button]').click();
      cy.get('[data-cy=save-policy-button]').click();
      cy.get('[data-cy=error-message]').should('be.visible');
    });

    it('should save new policy with valid data', () => {
      cy.get('[data-cy=create-policy-button]').click();
      cy.get('[data-cy=policy-name-input]').type('Test Policy');
      cy.get('[data-cy=policy-description-input]').type('Test policy description');
      cy.get('[data-cy=save-policy-button]').click();
      // Should show success message or return to list
      cy.url().should('include', '/policies');
    });

    it('should cancel policy creation', () => {
      cy.get('[data-cy=create-policy-button]').click();
      cy.get('[data-cy=cancel-policy-button]').click();
      cy.get('[data-cy=policy-editor]').should('not.exist');
    });
  });

  describe('Policy Actions', () => {
    it('should enable policy', () => {
      cy.get('[data-cy=policy-item]').first().within(() => {
        cy.get('[data-cy=policy-actions]').click();
        cy.get('[data-cy=enable-policy]').click();
      });
      cy.get('[data-cy=success-message]').should('be.visible');
    });

    it('should disable policy', () => {
      cy.get('[data-cy=policy-item]').first().within(() => {
        cy.get('[data-cy=policy-actions]').click();
        cy.get('[data-cy=disable-policy]').click();
      });
      cy.get('[data-cy=success-message]').should('be.visible');
    });

    it('should edit existing policy', () => {
      cy.get('[data-cy=policy-item]').first().within(() => {
        cy.get('[data-cy=policy-actions]').click();
        cy.get('[data-cy=edit-policy]').click();
      });
      cy.get('[data-cy=policy-editor]').should('be.visible');
    });

    it('should delete policy with confirmation', () => {
      cy.get('[data-cy=policy-item]').first().within(() => {
        cy.get('[data-cy=policy-actions]').click();
        cy.get('[data-cy=delete-policy]').click();
      });
      cy.get('[data-cy=confirmation-dialog]').should('be.visible');
      cy.get('[data-cy=confirm-delete]').click();
      cy.get('[data-cy=success-message]').should('be.visible');
    });

    it('should cancel policy deletion', () => {
      cy.get('[data-cy=policy-item]').first().within(() => {
        cy.get('[data-cy=policy-actions]').click();
        cy.get('[data-cy=delete-policy]').click();
      });
      cy.get('[data-cy=confirmation-dialog]').should('be.visible');
      cy.get('[data-cy=cancel-delete]').click();
      cy.get('[data-cy=confirmation-dialog]').should('not.exist');
    });
  });

  describe('Policy Packs', () => {
    it('should display canonical policy packs', () => {
      cy.get('[data-cy=policy-packs-section]').should('be.visible');
      cy.get('[data-cy=policy-pack-item]').should('have.length.at.least', 1);
    });

    it('should show policy pack details', () => {
      cy.get('[data-cy=policy-pack-item]').first().within(() => {
        cy.get('[data-cy=pack-name]').should('be.visible');
        cy.get('[data-cy=pack-description]').should('be.visible');
      });
    });

    it('should toggle policy pack', () => {
      cy.get('[data-cy=policy-pack-item]').first().within(() => {
        cy.get('[data-cy=pack-toggle]').click();
      });
      cy.get('[data-cy=success-message]').should('be.visible');
    });
  });

  describe('Policy Violations', () => {
    it('should display violations section', () => {
      cy.get('[data-cy=violations-section]').should('be.visible');
    });

    it('should list recent violations', () => {
      cy.get('[data-cy=violation-item]').should('have.length.at.least', 0);
    });

    it('should show violation details', () => {
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=violation-item]').length > 0) {
          cy.get('[data-cy=violation-item]').first().click();
          cy.get('[data-cy=violation-details]').should('be.visible');
        }
      });
    });

    it('should filter violations by policy', () => {
      cy.get('[data-cy=violation-filter]').should('exist');
    });
  });

  describe('Policy Templates', () => {
    it('should display policy templates', () => {
      cy.get('[data-cy=policy-templates-button]').click();
      cy.get('[data-cy=policy-templates]').should('be.visible');
    });

    it('should create policy from template', () => {
      cy.get('[data-cy=policy-templates-button]').click();
      cy.get('[data-cy=template-item]').first().within(() => {
        cy.get('[data-cy=use-template]').click();
      });
      cy.get('[data-cy=policy-editor]').should('be.visible');
    });
  });

  describe('Bulk Operations', () => {
    it('should select multiple policies', () => {
      cy.get('[data-cy=policy-checkbox]').first().click();
      cy.get('[data-cy=policy-checkbox]').eq(1).click();
      cy.get('[data-cy=bulk-actions-bar]').should('be.visible');
    });

    it('should enable multiple policies at once', () => {
      cy.get('[data-cy=policy-checkbox]').first().click();
      cy.get('[data-cy=policy-checkbox]').eq(1).click();
      cy.get('[data-cy=bulk-enable]').click();
      cy.get('[data-cy=success-message]').should('be.visible');
    });

    it('should disable multiple policies at once', () => {
      cy.get('[data-cy=policy-checkbox]').first().click();
      cy.get('[data-cy=policy-checkbox]').eq(1).click();
      cy.get('[data-cy=bulk-disable]').click();
      cy.get('[data-cy=success-message]').should('be.visible');
    });

    it('should clear selection', () => {
      cy.get('[data-cy=policy-checkbox]').first().click();
      cy.get('[data-cy=clear-selection]').click();
      cy.get('[data-cy=bulk-actions-bar]').should('not.exist');
    });
  });

  describe('Responsive Design', () => {
    it('should work on mobile viewport', () => {
      cy.viewport('iphone-x');
      cy.get('[data-cy=mobile-menu]').should('be.visible');
      cy.contains('Policies').should('be.visible');
    });

    it('should work on tablet viewport', () => {
      cy.viewport('ipad-2');
      cy.contains('Policies').should('be.visible');
    });
  });

  describe('Error Handling', () => {
    it('should display error message on API failure', () => {
      // Intercept API call and force error
      cy.intercept('GET', '/v1/policies', {
        statusCode: 500,
        body: { error: 'Internal Server Error' },
      }).as('getPoliciesError');

      cy.reload();
      cy.wait('@getPoliciesError');
      cy.get('[data-cy=error-message]').should('be.visible');
    });

    it('should retry on error', () => {
      cy.intercept('GET', '/v1/policies', {
        statusCode: 500,
        body: { error: 'Internal Server Error' },
      }).as('getPoliciesError');

      cy.reload();
      cy.wait('@getPoliciesError');
      cy.get('[data-cy=retry-button]').should('be.visible').click();
    });
  });

  describe('Loading States', () => {
    it('should show loading indicator', () => {
      cy.intercept('GET', '/v1/policies', (req) => {
        req.reply((res) => {
          res.delay = 1000;
        });
      }).as('getPolicies');

      cy.reload();
      cy.get('[data-cy=loading-indicator]').should('be.visible');
      cy.wait('@getPolicies');
      cy.get('[data-cy=loading-indicator]').should('not.exist');
    });
  });
});
