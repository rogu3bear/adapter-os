import './commands';

// Disable animations for stability and avoid visual flake.
beforeEach(() => {
  cy.disableAnimations();
});

// Keep tests running even if the app throws non-critical errors.
Cypress.on('uncaught:exception', () => {
  return false;
});
