// Custom Cypress commands and helpers for AdapterOS E2E runs.
// These helpers avoid hard-coded waits by relying on env-driven URLs and fast DOM hooks.

/// <reference types="cypress" />

const disableAnimationStyles = `
  *,
  *::before,
  *::after {
    transition: none !important;
    animation: none !important;
    scroll-behavior: auto !important;
    caret-color: transparent !important;
  }
`;

export const resolveBaseUrl = (): string => {
  return (
    (Cypress.env('baseUrl') as string | undefined) ||
    (Cypress.env('BASE_URL') as string | undefined) ||
    (Cypress.env('CYPRESS_baseUrl') as string | undefined) ||
    (Cypress.env('CYPRESS_BASE_URL') as string | undefined) ||
    Cypress.config('baseUrl')
  );
};

export const resolveApiUrl = (): string | undefined => {
  return (
    (Cypress.env('API_URL') as string | undefined) ||
    (Cypress.env('API_BASE_URL') as string | undefined)
  );
};

Cypress.Commands.add('disableAnimations', () => {
  cy.document().then((doc) => {
    if (doc.getElementById('cypress-disable-animations')) {
      return;
    }
    const style = doc.createElement('style');
    style.id = 'cypress-disable-animations';
    style.innerHTML = disableAnimationStyles;
    doc.head.appendChild(style);
  });
});

Cypress.Commands.add('loginWithToken', (token?: string) => {
  const authToken =
    token ||
    (Cypress.env('AUTH_TOKEN') as string | undefined) ||
    (Cypress.env('AOS_AUTH_TOKEN') as string | undefined);

  if (!authToken) {
    Cypress.log({
      name: 'loginWithToken',
      message: 'No auth token provided; skipping token injection',
    });
    return cy.wrap(null);
  }

  return cy.window().then((win) => {
    win.localStorage.setItem('aos_auth_token', authToken);
  });
});

declare global {
  namespace Cypress {
    interface Chainable {
      disableAnimations(): Chainable<void>;
      loginWithToken(token?: string): Chainable<void>;
    }
  }
}
