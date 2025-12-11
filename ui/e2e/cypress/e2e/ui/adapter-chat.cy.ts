/// <reference types="cypress" />

const api = Cypress.env('API_BASE_URL') || 'http://localhost:8080';
const tenantId = Cypress.env('TEST_TENANT_ID') || 'tenant-test';
const modelId = Cypress.env('TEST_MODEL_ID') || 'model-qwen-test';
const adapterId = Cypress.env('TEST_ADAPTER_ID') || 'adapter-test';
const stackId = Cypress.env('TEST_STACK_ID') || 'stack-test';

describe('Adapter chat happy path (stubbed)', () => {
  beforeEach(() => {
    cy.seedTestData({ skipReset: false, chat: true });

    cy.visit('/', {
      onBeforeLoad(win) {
        win.localStorage.setItem('selectedTenant', tenantId);
        win.localStorage.setItem('aos-auth-active', 'true');
      },
    });
  });

  it('shows seeded model, adapter, stack, and seeded chat history without stubs', () => {
    // Models page should render seeded base model
    cy.visit('/models');
    cy.contains(modelId).should('exist');
    cy.contains('q4_0').should('exist');

    // Adapters page should list seeded adapter
    cy.visit('/adapters');
    cy.contains(adapterId).should('exist');
    cy.contains('Test Adapter').should('exist');

    // Chat page should load seeded session/message
    cy.visit('/chat');
    cy.contains('stack.test').should('exist');
    cy.contains('Seeded Cypress Session').click({ force: true });
    cy.get('[aria-label="Chat messages"]').within(() => {
      cy.contains('Hello from seeded fixtures').should('exist');
    });
  });
});

