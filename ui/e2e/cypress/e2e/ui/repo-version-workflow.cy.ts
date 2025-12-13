/// <reference types="cypress" />
import '../../support/commands';

describe('Adapter repo + version workflow', () => {
  const apiBase = (Cypress.env('API_BASE_URL') as string | undefined) || 'http://localhost:8080';
  const repoId = 'repo-e2e-deterministic';
  const versionId = 'ver-e2e-1';
  const versionTag = '1.0.0';

  beforeEach(() => {
    cy.login();

    const state: {
      repos: any[];
      versions: Record<string, any[]>;
    } = {
      repos: [],
      versions: {},
    };

    const findRepo = (id: string) => state.repos.find((r) => r.id === id);

    cy.intercept('GET', '**/v1/repos', (req) => {
      req.reply({ statusCode: 200, body: state.repos });
    }).as('listRepos');

    cy.intercept('POST', '**/v1/repos', (req) => {
      const body = req.body || {};
      const repo = {
        id: repoId,
        name: body.name ?? repoId,
        base_model: body.base_model ?? 'qwen2.5-7b',
        default_branch: body.default_branch ?? 'main',
        status: 'healthy',
        branches: [
          { name: 'main', default: true, latest_active_version: null },
          { name: 'dev', default: false, latest_active_version: null },
        ],
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
        tenant_id: 'tenant-e2e',
      };
      state.repos = [repo];
      state.versions[repo.id] = [];
      req.reply({ statusCode: 201, body: repo });
    }).as('createRepo');

    cy.intercept('GET', '**/v1/repos/*/versions/*', (req) => {
      const match = req.url.match(/\/v1\/repos\/([^/]+)\/versions\/([^/]+)/);
      const repo = match?.[1];
      const version = match?.[2];
      const found = repo ? state.versions[repo]?.find((v) => v.id === version) : undefined;
      if (found) {
        req.reply({ statusCode: 200, body: found });
      } else {
        req.reply({ statusCode: 404, body: { error: 'not found' } });
      }
    }).as('getRepoVersion');

    cy.intercept('GET', '**/v1/repos/*/versions', (req) => {
      const match = req.url.match(/\/v1\/repos\/([^/]+)\//);
      const repo = match?.[1];
      req.reply({ statusCode: 200, body: repo ? state.versions[repo] ?? [] : [] });
    }).as('listRepoVersions');

    cy.intercept('POST', '**/v1/repos/*/versions', (req) => {
      const match = req.url.match(/\/v1\/repos\/([^/]+)\//);
      const repo = match?.[1];
      const body = req.body || {};
      const newVersion = {
        id: versionId,
        version: body.version ?? versionTag,
        branch: body.branch ?? 'main',
        release_state: body.release_state ?? 'candidate',
        serveable: body.serveable ?? true,
        serveable_reason: body.serveable_reason,
        aos_hash: body.aos_hash ?? 'hash-e2e-stable',
        commit_sha: body.commit_sha ?? 'abc123stable',
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      };
      if (repo) {
        state.versions[repo] = [newVersion];
      }
      req.reply({ statusCode: 201, body: newVersion });
    }).as('createRepoVersion');

    cy.intercept('POST', '**/v1/repos/*/versions/*/promote', (req) => {
      const match = req.url.match(/\/v1\/repos\/([^/]+)\/versions\/([^/]+)\//);
      const repo = match?.[1];
      const version = match?.[2];
      const found = repo ? state.versions[repo]?.find((v) => v.id === version) : undefined;
      if (found) {
        found.release_state = 'active';
        found.updated_at = new Date().toISOString();
        const repoState = repo ? findRepo(repo) : undefined;
        if (repoState) {
          repoState.branches = repoState.branches.map((b: any) =>
            b.name === found.branch ? { ...b, latest_active_version: found } : b,
          );
        }
        req.reply({ statusCode: 200, body: found });
      } else {
        req.reply({ statusCode: 404, body: { error: 'not found' } });
      }
    }).as('promoteRepoVersion');

    cy.intercept('PATCH', '**/v1/repos/*', (req) => {
      const match = req.url.match(/\/v1\/repos\/([^/]+)$/);
      const repo = match?.[1];
      const body = req.body || {};
      const repoState = repo ? findRepo(repo) : undefined;
      if (repoState) {
        if (body.default_branch) {
          repoState.default_branch = body.default_branch;
          repoState.branches = repoState.branches.map((b: any) => ({
            ...b,
            default: b.name === body.default_branch,
          }));
        }
        repoState.updated_at = new Date().toISOString();
        req.reply({ statusCode: 200, body: repoState });
      } else {
        req.reply({ statusCode: 404, body: { error: 'not found' } });
      }
    }).as('updateRepo');

    cy.intercept('GET', '**/v1/repos/*/timeline', { statusCode: 200, body: [] }).as('getRepoTimeline');
    cy.intercept('GET', '**/v1/repos/*/training-jobs', { statusCode: 200, body: [] }).as('getRepoTrainingJobs');

    cy.intercept('GET', '**/v1/repos/*', (req) => {
      const match = req.url.match(/\/v1\/repos\/([^/]+)$/);
      const repo = match?.[1];
      const found = repo ? findRepo(repo) : undefined;
      if (found) {
        req.reply({ statusCode: 200, body: found });
      } else {
        req.reply({ statusCode: 404, body: { error: 'not found' } });
      }
    }).as('getRepo');
  });

  it('creates repo, drafts version, promotes, and keeps metadata stable', () => {
    cy.visit('/repos');
    cy.wait('@listRepos');

    cy.get('[data-cy=create-repo-btn]').click();
    cy.get('[data-cy=repo-name-input]').clear().type(repoId);
    cy.get('[data-cy=repo-base-model-input]').clear().type('qwen2.5-7b');
    cy.get('[data-cy=repo-default-branch-input]').clear().type('main');
    cy.get('[data-cy=repo-create-submit]').click();

    cy.wait(['@createRepo', '@getRepo', '@listRepoVersions', '@getRepoTimeline', '@getRepoTrainingJobs']);
    cy.url().should('include', `/repos/${repoId}`);

    // Change default branch to dev to prove branch updates persist
    cy.get('[data-cy=repo-branch-trigger]').click();
    cy.contains('[role=option]', 'dev').click();
    cy.wait('@updateRepo');
    cy.get('[data-cy=repo-branch-trigger]').contains('dev');

    // Create a version via API (intercept-backed)
    cy.request('POST', `${apiBase}/v1/repos/${repoId}/versions`, {
      version: versionTag,
      branch: 'dev',
      aos_hash: 'hash-e2e-stable',
      commit_sha: 'abc123stable',
      release_state: 'candidate',
      serveable: true,
    });
    cy.wait('@createRepoVersion');

    // Refresh to validate list re-fetch uses cached state and survives reload
    cy.reload();
    cy.wait(['@getRepo', '@listRepoVersions']);

    cy.get(`[data-cy=repo-version-row-${versionId}]`).should('exist');
    cy.get(`[data-cy=repo-version-release-${versionId}]`).should('contain', 'candidate');

    // Promote and ensure release_state updates without manual hacks
    cy.get(`[data-cy=version-promote-${versionId}]`).click();
    cy.wait('@promoteRepoVersion');
    cy.get(`[data-cy=repo-version-release-${versionId}]`).should('contain', 'active');

    // Open detail to verify hashes stay stable across refresh
    cy.get(`[data-cy=version-view-${versionId}]`).click();
    cy.wait('@getRepoVersion');
    cy.get('[data-cy=version-aos-hash]').should('contain', 'hash-e2e-stable');
    cy.get('[data-cy=version-commit-sha]').should('contain', 'abc123stable');
    cy.get('[data-cy=version-release-state]').should('contain', 'active');

    cy.reload();
    cy.wait('@getRepoVersion');
    cy.get('[data-cy=version-aos-hash]').should('contain', 'hash-e2e-stable');
    cy.get('[data-cy=version-release-state]').should('contain', 'active');
  });
});
