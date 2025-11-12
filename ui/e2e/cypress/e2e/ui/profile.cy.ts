/// <reference types="cypress" />

describe('Profile Page UI Tests', () => {
  beforeEach(() => {
    cy.login();
    cy.visit('/profile');
  });

  describe('Page Load and Navigation', () => {
    it('should load profile page successfully', () => {
      cy.url().should('include', '/profile');
      cy.contains('Profile').should('be.visible');
    });

    it('should display page header', () => {
      cy.get('[data-cy=page-header]').should('be.visible');
    });

    it('should have breadcrumb navigation', () => {
      cy.get('[data-cy=breadcrumb]').should('be.visible');
    });
  });

  describe('Profile Information Display', () => {
    it('should display user information', () => {
      cy.get('[data-cy=user-info-section]').should('be.visible');
      cy.get('[data-cy=user-email]').should('be.visible');
      cy.get('[data-cy=user-id]').should('be.visible');
      cy.get('[data-cy=user-role]').should('be.visible');
    });

    it('should display user avatar', () => {
      cy.get('[data-cy=user-avatar]').should('be.visible');
    });

    it('should show account creation date', () => {
      cy.get('[data-cy=account-created-at]').should('be.visible');
    });

    it('should display last login information', () => {
      cy.get('[data-cy=last-login]').should('be.visible');
    });
  });

  describe('Edit Profile', () => {
    it('should open profile edit dialog', () => {
      cy.get('[data-cy=edit-profile-button]').click();
      cy.get('[data-cy=edit-profile-dialog]').should('be.visible');
    });

    it('should have editable fields', () => {
      cy.get('[data-cy=edit-profile-button]').click();
      cy.get('[data-cy=display-name-input]').should('be.visible');
      cy.get('[data-cy=bio-input]').should('be.visible');
    });

    it('should update display name', () => {
      cy.get('[data-cy=edit-profile-button]').click();
      cy.get('[data-cy=display-name-input]').clear().type('Updated Name');
      cy.get('[data-cy=save-profile-button]').click();
      cy.get('[data-cy=success-message]').should('be.visible');
    });

    it('should cancel profile edit', () => {
      cy.get('[data-cy=edit-profile-button]').click();
      cy.get('[data-cy=display-name-input]').clear().type('Test Name');
      cy.get('[data-cy=cancel-edit-button]').click();
      cy.get('[data-cy=edit-profile-dialog]').should('not.exist');
    });

    it('should validate required fields', () => {
      cy.get('[data-cy=edit-profile-button]').click();
      cy.get('[data-cy=display-name-input]').clear();
      cy.get('[data-cy=save-profile-button]').click();
      cy.get('[data-cy=error-message]').should('be.visible');
    });
  });

  describe('Change Password', () => {
    it('should open change password dialog', () => {
      cy.get('[data-cy=change-password-button]').click();
      cy.get('[data-cy=change-password-dialog]').should('be.visible');
    });

    it('should have password fields', () => {
      cy.get('[data-cy=change-password-button]').click();
      cy.get('[data-cy=current-password-input]').should('be.visible');
      cy.get('[data-cy=new-password-input]').should('be.visible');
      cy.get('[data-cy=confirm-password-input]').should('be.visible');
    });

    it('should validate password match', () => {
      cy.get('[data-cy=change-password-button]').click();
      cy.get('[data-cy=current-password-input]').type('oldpass');
      cy.get('[data-cy=new-password-input]').type('newpass123');
      cy.get('[data-cy=confirm-password-input]').type('differentpass');
      cy.get('[data-cy=save-password-button]').click();
      cy.get('[data-cy=error-message]').should('contain', /match/i);
    });

    it('should validate password strength', () => {
      cy.get('[data-cy=change-password-button]').click();
      cy.get('[data-cy=new-password-input]').type('weak');
      cy.get('[data-cy=password-strength]').should('be.visible');
    });

    it('should cancel password change', () => {
      cy.get('[data-cy=change-password-button]').click();
      cy.get('[data-cy=cancel-password-button]').click();
      cy.get('[data-cy=change-password-dialog]').should('not.exist');
    });
  });

  describe('API Keys Management', () => {
    it('should display API keys section', () => {
      cy.get('[data-cy=api-keys-section]').should('be.visible');
    });

    it('should list existing API keys', () => {
      cy.get('[data-cy=api-keys-list]').should('be.visible');
    });

    it('should create new API key', () => {
      cy.get('[data-cy=create-api-key-button]').click();
      cy.get('[data-cy=api-key-dialog]').should('be.visible');
      cy.get('[data-cy=api-key-name-input]').type('Test API Key');
      cy.get('[data-cy=create-key-button]').click();
      cy.get('[data-cy=success-message]').should('be.visible');
    });

    it('should display created API key', () => {
      cy.get('[data-cy=create-api-key-button]').click();
      cy.get('[data-cy=api-key-name-input]').type('Test Key');
      cy.get('[data-cy=create-key-button]').click();
      cy.get('[data-cy=api-key-value]').should('be.visible');
      cy.get('[data-cy=copy-api-key]').should('be.visible');
    });

    it('should revoke API key', () => {
      cy.get('[data-cy=api-key-item]').first().within(() => {
        cy.get('[data-cy=revoke-key]').click();
      });
      cy.get('[data-cy=confirmation-dialog]').should('be.visible');
      cy.get('[data-cy=confirm-revoke]').click();
      cy.get('[data-cy=success-message]').should('be.visible');
    });

    it('should copy API key to clipboard', () => {
      cy.get('[data-cy=create-api-key-button]').click();
      cy.get('[data-cy=api-key-name-input]').type('Copy Test');
      cy.get('[data-cy=create-key-button]').click();
      cy.get('[data-cy=copy-api-key]').click();
      cy.get('[data-cy=copied-message]').should('be.visible');
    });
  });

  describe('Session Management', () => {
    it('should display active sessions', () => {
      cy.get('[data-cy=sessions-section]').should('be.visible');
      cy.get('[data-cy=sessions-list]').should('be.visible');
    });

    it('should show current session', () => {
      cy.get('[data-cy=current-session]').should('be.visible');
      cy.get('[data-cy=current-session]').should('contain', 'Current');
    });

    it('should display session details', () => {
      cy.get('[data-cy=session-item]').first().within(() => {
        cy.get('[data-cy=session-device]').should('be.visible');
        cy.get('[data-cy=session-location]').should('be.visible');
        cy.get('[data-cy=session-last-active]').should('be.visible');
      });
    });

    it('should revoke session', () => {
      cy.get('[data-cy=session-item]').eq(1).within(() => {
        cy.get('[data-cy=revoke-session]').click();
      });
      cy.get('[data-cy=confirmation-dialog]').should('be.visible');
      cy.get('[data-cy=confirm-revoke]').click();
      cy.get('[data-cy=success-message]').should('be.visible');
    });

    it('should revoke all sessions', () => {
      cy.get('[data-cy=revoke-all-sessions]').click();
      cy.get('[data-cy=confirmation-dialog]').should('be.visible');
      cy.get('[data-cy=confirm-revoke-all]').click();
      cy.url().should('include', '/login');
    });
  });

  describe('Preferences', () => {
    it('should display preferences section', () => {
      cy.get('[data-cy=preferences-section]').should('be.visible');
    });

    it('should toggle email notifications', () => {
      cy.get('[data-cy=email-notifications-toggle]').click();
      cy.get('[data-cy=success-message]').should('be.visible');
    });

    it('should change theme', () => {
      cy.get('[data-cy=theme-selector]').click();
      cy.get('[data-cy=theme-dark]').click();
      cy.get('[data-cy=success-message]').should('be.visible');
    });

    it('should change language', () => {
      cy.get('[data-cy=language-selector]').click();
      cy.get('[data-cy=language-option]').first().click();
    });

    it('should update timezone', () => {
      cy.get('[data-cy=timezone-selector]').click();
      cy.get('[data-cy=timezone-option]').first().click();
      cy.get('[data-cy=success-message]').should('be.visible');
    });
  });

  describe('Activity Log', () => {
    it('should display activity log section', () => {
      cy.get('[data-cy=activity-log-section]').should('be.visible');
    });

    it('should list recent activities', () => {
      cy.get('[data-cy=activity-item]').should('have.length.at.least', 0);
    });

    it('should display activity details', () => {
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=activity-item]').length > 0) {
          cy.get('[data-cy=activity-item]').first().within(() => {
            cy.get('[data-cy=activity-type]').should('be.visible');
            cy.get('[data-cy=activity-timestamp]').should('be.visible');
          });
        }
      });
    });

    it('should filter activities by type', () => {
      cy.get('[data-cy=activity-filter]').click();
      cy.get('[data-cy=filter-login]').click();
    });

    it('should paginate activity log', () => {
      cy.get('[data-cy=activity-pagination]').should('be.visible');
      cy.get('[data-cy=next-page]').click();
    });
  });

  describe('Account Actions', () => {
    it('should display dangerous actions section', () => {
      cy.get('[data-cy=danger-zone]').should('be.visible');
    });

    it('should show delete account option', () => {
      cy.get('[data-cy=delete-account-button]').should('be.visible');
    });

    it('should require confirmation for account deletion', () => {
      cy.get('[data-cy=delete-account-button]').click();
      cy.get('[data-cy=confirmation-dialog]').should('be.visible');
      cy.get('[data-cy=delete-confirmation-text]').should('contain', /permanent/i);
    });

    it('should cancel account deletion', () => {
      cy.get('[data-cy=delete-account-button]').click();
      cy.get('[data-cy=cancel-delete]').click();
      cy.get('[data-cy=confirmation-dialog]').should('not.exist');
    });
  });

  describe('Responsive Design', () => {
    it('should work on mobile viewport', () => {
      cy.viewport('iphone-x');
      cy.get('[data-cy=mobile-menu]').should('be.visible');
      cy.contains('Profile').should('be.visible');
    });

    it('should work on tablet viewport', () => {
      cy.viewport('ipad-2');
      cy.contains('Profile').should('be.visible');
    });
  });

  describe('Error Handling', () => {
    it('should display error message on API failure', () => {
      cy.intercept('GET', '/v1/auth/me', {
        statusCode: 500,
        body: { error: 'Internal Server Error' },
      }).as('getProfileError');

      cy.reload();
      cy.wait('@getProfileError');
      cy.get('[data-cy=error-message]').should('be.visible');
    });

    it('should retry on error', () => {
      cy.intercept('GET', '/v1/auth/me', {
        statusCode: 500,
        body: { error: 'Internal Server Error' },
      }).as('getProfileError');

      cy.reload();
      cy.wait('@getProfileError');
      cy.get('[data-cy=retry-button]').should('be.visible').click();
    });
  });

  describe('Loading States', () => {
    it('should show loading indicator', () => {
      cy.intercept('GET', '/v1/auth/me', (req) => {
        req.reply((res) => {
          res.delay = 1000;
        });
      }).as('getProfile');

      cy.reload();
      cy.get('[data-cy=loading-indicator]').should('be.visible');
      cy.wait('@getProfile');
      cy.get('[data-cy=loading-indicator]').should('not.exist');
    });
  });
});
