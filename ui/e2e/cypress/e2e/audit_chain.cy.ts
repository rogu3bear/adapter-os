describe('Audit chain integrity', () => {
  const tenantId = 'tenant-test';
  const isE2E = Cypress.env('E2E_MODE') === '1';

  const togglePolicy = (enabled: boolean) =>
    cy.apiRequest({
      method: 'POST',
      url: `/v1/tenants/${tenantId}/policy-bindings/determinism/toggle`,
      body: { enabled },
      failOnStatusCode: false,
    });

  beforeEach(() => {
    if (!isE2E) return;
    cy.login();
    cy.apiRequest({ method: 'POST', url: '/testkit/seed_minimal' });
  });

  it('shows policy audit chain with previous hashes', () => {
    if (!isE2E) {
      cy.log('E2E_MODE not enabled; skipping audit chain test');
      return;
    }

    togglePolicy(true);
    togglePolicy(false);

    cy.visit('/security/audit');
    cy.get('[data-cy=policy-audit-row]').should('have.length.at.least', 2);
    cy.get('[data-cy=policy-audit-row]').then(($rows) => {
      const seqs = [...$rows].map((row) => Number(row.getAttribute('data-seq')));
      const isMonotonic = seqs.every((s, idx) => idx === 0 || s >= seqs[idx - 1]);
      expect(isMonotonic).to.eq(true);
    });
    cy.get('[data-cy=policy-audit-row]').eq(1).within(() => {
      cy.get('td').eq(2).invoke('text').should('not.contain', '—');
    });
  });

  it('blocks policy writes when audit chain diverges', () => {
    if (!isE2E) {
      cy.log('E2E_MODE not enabled; skipping divergence test');
      return;
    }

    cy.apiRequest({
      method: 'POST',
      url: `/testkit/audit/diverge?tenant_id=${tenantId}`,
    }).its('status').should('eq', 200);

    togglePolicy(true).then((response) => {
      expect(response.status).to.eq(409);
      expect(response.body?.code).to.eq('AUDIT_CHAIN_DIVERGED');
    });

    cy.visit('/security/audit');
    cy.get('[data-cy=audit-chain-status]').should('contain.text', 'Chain verification failed');
  });
});
