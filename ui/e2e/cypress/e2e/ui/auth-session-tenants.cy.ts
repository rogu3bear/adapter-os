/// <reference types="cypress" />

type TenantSummary = { id: string; name: string };

const credentials = {
  email: Cypress.env('TEST_USER_EMAIL') || 'test@example.com',
  password: Cypress.env('TEST_USER_PASSWORD') || 'password',
};

const tenants: { primary: TenantSummary | null; secondary: TenantSummary | null } = {
  primary: null,
  secondary: null,
};

let dataReady = false;
let repoAName = '';
let repoBName = '';

function extractTenants(body: any): TenantSummary[] {
  if (!body) return [];
  if (Array.isArray(body)) return body as TenantSummary[];
  if (Array.isArray(body.tenants)) return body.tenants as TenantSummary[];
  return [];
}

function switchTenantApi(tenantId: string) {
  return cy
    .apiRequest<{ token?: string }>({
      method: 'POST',
      url: '/v1/auth/tenants/switch',
      body: { tenant_id: tenantId },
      failOnStatusCode: false,
    })
    .then((resp) => {
      expect(resp.status).to.be.oneOf([200, 201]);
      if (resp.body?.token) {
        Cypress.env('authToken', resp.body.token);
      }
      return resp;
    });
}

function createRepoForTenant(tenantId: string, name: string) {
  return switchTenantApi(tenantId).then(() =>
    cy
      .apiRequest({
        method: 'POST',
        url: '/v1/repos',
        body: {
          name,
          base_model: 'qwen2.5-7b',
          default_branch: 'main',
        },
        failOnStatusCode: false,
      })
      .then((resp) => {
        expect(resp.status).to.be.oneOf([200, 201, 409]);
      })
  );
}

function ensureTenants() {
  return cy
    .apiRequest({
      method: 'GET',
      url: '/v1/auth/tenants',
      failOnStatusCode: false,
    })
    .then((resp) => {
      const list = extractTenants(resp.body);
      if (list.length >= 2) {
        tenants.primary = list[0];
        tenants.secondary = list[1];
        return;
      }

      const name = `cypress-tenant-${Date.now()}`;
      return cy
        .apiRequest({
          method: 'POST',
          url: '/v1/tenants',
          body: { name },
          failOnStatusCode: false,
        })
        .then((createResp) => {
          if (![200, 201].includes(createResp.status)) {
            cy.log(`Unable to create secondary tenant (status ${createResp.status})`);
            return;
          }
          return cy
            .apiRequest({
              method: 'GET',
              url: '/v1/auth/tenants',
              failOnStatusCode: false,
            })
            .then((secondResp) => {
              const finalList = extractTenants(secondResp.body);
              if (finalList.length >= 2) {
                tenants.primary = finalList[0];
                tenants.secondary = finalList[1];
              }
            });
        });
    });
}

function uiLogin(selectedTenantId?: string) {
  cy.clearCookies();
  cy.clearLocalStorage();

  cy.visit('/login', {
    onBeforeLoad(win) {
      if (selectedTenantId) {
        win.localStorage.setItem('selectedTenant', selectedTenantId);
      }
    },
  });

  cy.get('input#email', { timeout: 20000 }).should('be.visible').type(credentials.email);
  cy.get('input#password', { log: false }).type(credentials.password, { log: false });
  cy.contains('button', 'Sign in', { matchCase: false }).click();
  cy.url({ timeout: 20000 }).should('include', '/dashboard');
}

describe('Auth, session, and tenant isolation', () => {
  before(() => {
    cy.login().then(() =>
      ensureTenants().then(() => {
        if (!tenants.primary || !tenants.secondary) {
          cy.log('Skipping tenant isolation prep (not enough tenants)');
          return;
        }

        repoAName = `cypress-repo-${Date.now()}-a`;
        repoBName = `cypress-repo-${Date.now()}-b`;

        return createRepoForTenant(tenants.primary.id, repoAName)
          .then(() => createRepoForTenant(tenants.secondary!.id, repoBName))
          .then(() => switchTenantApi(tenants.primary!.id))
          .then(() => {
            dataReady = true;
          });
      })
    );
  });

  beforeEach(() => {
    cy.clearCookies();
    cy.clearLocalStorage();
  });

  it('Login success sets session cookie and redirects home', () => {
    uiLogin(tenants.primary?.id || undefined);
    cy.url().should('include', '/dashboard');
    cy.getCookie('auth_token').should('exist');
  });

  it('Login failure shows error and stays on login', () => {
    cy.visit('/login');
    cy.get('input#email').type(credentials.email);
    cy.get('input#password').type('wrong-password', { log: false });
    cy.contains('button', 'Sign in', { matchCase: false }).click();
    cy.url().should('include', '/login');
    cy.contains(/invalid email or password|login failed/i);
    cy.getCookie('auth_token').should('not.exist');
  });

  it('Refresh survives navigation', () => {
    uiLogin(tenants.primary?.id || undefined);
    cy.visit('/repos');
    cy.contains('Repositories', { timeout: 20000 }).should('be.visible');
    cy.reload();
    cy.url().should('include', '/repos');
    cy.contains('Repositories').should('be.visible');
    cy.getCookie('auth_token').should('exist');
  });

  it('Logout clears session and blocks protected routes', () => {
    uiLogin(tenants.primary?.id || undefined);
    cy.get('[data-cy=tenant-switcher]', { timeout: 20000 }).should('be.visible');
    cy.get('[data-cy=user-menu-trigger]').click();
    cy.get('[data-cy=logout-action]').click();

    cy.url({ timeout: 20000 }).should('include', '/login');
    cy.getCookie('auth_token').should('not.exist');

    cy.visit('/repos');
    cy.url().should('include', '/login');
  });

  it('Tenant switch isolates repos and audit logs', function () {
    if (!dataReady || !tenants.primary || !tenants.secondary) {
      this.skip();
    }

    uiLogin(tenants.primary.id);

    cy.intercept('GET', '**/v1/repos*').as('repos');
    cy.intercept('GET', '**/v1/audit/logs*').as('auditLogs');

    cy.visit('/repos');
    cy.wait('@repos');
    cy.contains(repoAName, { timeout: 20000 }).should('be.visible');

    cy.visit('/security/audit');
    cy.wait('@auditLogs').then(({ response }) => {
      const logs = Array.isArray(response?.body)
        ? response?.body
        : Array.isArray((response?.body as any)?.items)
          ? (response?.body as any).items
          : [];
      if (logs.length > 0) {
        const tenantIds = Array.from(new Set(logs.map((l: any) => l.tenant_id).filter(Boolean)));
        expect(tenantIds).to.have.length(1);
        expect(tenantIds[0]).to.eq(tenants.primary!.id);
      }
    });

    cy.visit('/repos');
    cy.get('[data-cy=tenant-switcher]').click();
    cy.get(`[data-tenant-id="${tenants.secondary.id}"]`, { timeout: 10000 }).click();

    cy.wait('@repos');
    cy.contains(repoBName, { timeout: 20000 }).should('be.visible');
    cy.contains(repoAName).should('not.exist');

    cy.visit('/security/audit');
    cy.wait('@auditLogs').then(({ response }) => {
      const logs = Array.isArray(response?.body)
        ? response?.body
        : Array.isArray((response?.body as any)?.items)
          ? (response?.body as any).items
          : [];
      if (logs.length > 0) {
        const tenantIds = Array.from(new Set(logs.map((l: any) => l.tenant_id).filter(Boolean)));
        expect(tenantIds).to.have.length(1);
        expect(tenantIds[0]).to.eq(tenants.secondary!.id);
      }
    });
  });
});

