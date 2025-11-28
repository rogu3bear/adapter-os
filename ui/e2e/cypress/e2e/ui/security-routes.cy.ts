/**
 * Security Routes Navigation Test
 * Tests the Security group navigation routes:
 * - /security/policies
 * - /security/audit
 * - /security/compliance
 */

describe('Security Routes', () => {
  beforeEach(() => {
    // Visit the base URL
    cy.visit('/');
  });

  it('should navigate to Policies page', () => {
    cy.visit('/security/policies');
    cy.url().should('include', '/security/policies');

    // Check for page header or title
    cy.contains('Policies', { timeout: 10000 }).should('be.visible');

    // Take a screenshot
    cy.screenshot('policies-page');
  });

  it('should navigate to Audit page', () => {
    cy.visit('/security/audit');
    cy.url().should('include', '/security/audit');

    // Check for page header or title
    cy.contains('Audit', { timeout: 10000 }).should('be.visible');

    // Take a screenshot
    cy.screenshot('audit-page');
  });

  it('should navigate to Compliance page', () => {
    cy.visit('/security/compliance');
    cy.url().should('include', '/security/compliance');

    // Check for page header or title
    cy.contains('Compliance', { timeout: 10000 }).should('be.visible');

    // Take a screenshot
    cy.screenshot('compliance-page');
  });

  it('should not show any console errors on Security pages', () => {
    const routes = ['/security/policies', '/security/audit', '/security/compliance'];

    routes.forEach((route) => {
      cy.visit(route);
      cy.wait(2000); // Wait for page to fully load

      // Check window for any errors (this is a basic check)
      cy.window().then((win) => {
        expect(win.console.error).to.not.have.been.called;
      });
    });
  });
});
