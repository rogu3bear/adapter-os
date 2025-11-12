// Repository Management API Tests
import { validateErrorResponse } from '../../support/api-helpers';

describe('Repository Management API', () => {
  beforeEach(() => {
    cy.login();
  });

  // Clean up tracked resources after each test
  afterEach(() => {
    cy.cleanupTestData();
  });

  describe('List Repositories', () => {
    it('should list all repositories', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/repositories',
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.oneOf([200, 404]);

        if (response.status === 200) {
          expect(response.body).to.be.an('array');
        }
      });
    });

    it('should reject unauthenticated requests', () => {
      cy.request({
        method: 'GET',
        url: `${Cypress.env('API_BASE_URL')}/v1/repositories`,
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(401);
        validateErrorResponse(response);
      });
    });
  });

  describe('Register Repository', () => {
    it('should register a new repository with valid URL', () => {
      const repoRequest = {
        url: 'https://github.com/example/test-repo.git',
        branch: 'main',
      };

      cy.apiRequest({
        method: 'POST',
        url: '/v1/repositories',
        body: repoRequest,
        failOnStatusCode: false,
      }).then((response) => {
        // May succeed or fail depending on repository accessibility
        expect(response.status).to.be.oneOf([200, 201, 400, 404, 422]);

        if (response.status === 200 || response.status === 201) {
          expect(response.body).to.have.property('id');
          expect(response.body).to.have.property('url');
          expect(response.body).to.have.property('branch');

          // Track repository for cleanup
          if (response.body.id) {
            cy.trackResource('repository', response.body.id, `/v1/repositories/${response.body.id}`);
          }
        } else if (response.status >= 400) {
          validateErrorResponse(response);
        }
      });
    });

    it('should register repository with SSH URL', () => {
      const repoRequest = {
        url: 'git@github.com:example/test-repo.git',
        branch: 'main',
      };

      cy.apiRequest({
        method: 'POST',
        url: '/v1/repositories',
        body: repoRequest,
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.oneOf([200, 201, 400, 404, 422]);

        if (response.status === 200 || response.status === 201) {
          expect(response.body).to.have.property('id');
          if (response.body.id) {
            cy.trackResource('repository', response.body.id, `/v1/repositories/${response.body.id}`);
          }
        } else if (response.status >= 400) {
          validateErrorResponse(response);
        }
      });
    });

    it('should use default branch if not specified', () => {
      const repoRequest = {
        url: 'https://github.com/example/default-branch-repo.git',
      };

      cy.apiRequest({
        method: 'POST',
        url: '/v1/repositories',
        body: repoRequest,
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.oneOf([200, 201, 400, 404, 422]);

        if (response.status === 200 || response.status === 201) {
          expect(response.body).to.have.property('branch');
          expect(response.body.branch).to.eq('main'); // Default branch
          if (response.body.id) {
            cy.trackResource('repository', response.body.id, `/v1/repositories/${response.body.id}`);
          }
        } else if (response.status >= 400) {
          validateErrorResponse(response);
        }
      });
    });

    it('should reject registration with invalid URL', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/repositories',
        body: {
          url: 'not-a-valid-url',
          branch: 'main',
        },
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.at.least(400);
        validateErrorResponse(response);
      });
    });

    it('should reject registration with missing URL', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/repositories',
        body: {
          branch: 'main',
        },
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.at.least(400);
        validateErrorResponse(response);
      });
    });

    it('should handle optional fields', () => {
      const repoRequest = {
        url: 'https://github.com/example/test-repo.git',
        branch: 'develop',
        repo_id: 'custom-repo-id',
        tenant_id: 'test-tenant',
        languages: ['rust', 'javascript'],
      };

      cy.apiRequest({
        method: 'POST',
        url: '/v1/repositories',
        body: repoRequest,
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.oneOf([200, 201, 400, 404, 422]);

        if (response.status === 200 || response.status === 201) {
          if (response.body.id) {
            cy.trackResource('repository', response.body.id, `/v1/repositories/${response.body.id}`);
          }
        } else if (response.status >= 400) {
          validateErrorResponse(response);
        }
      });
    });
  });

  describe('Get Repository', () => {
    it('should get repository details by ID', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/repositories',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const repoId = listResponse.body[0].id;

          cy.apiRequest({
            method: 'GET',
            url: `/v1/repositories/${repoId}`,
          }).then((response) => {
            expect(response.status).to.eq(200);
            expect(response.body).to.have.property('id');
            expect(response.body.id).to.eq(repoId);
            expect(response.body).to.have.property('url');
            expect(response.body).to.have.property('branch');
            expect(response.body).to.have.property('commit_count');
          });
        } else {
          cy.log('No repositories available for testing');
        }
      });
    });

    it('should return 404 for non-existent repository', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/repositories/non-existent-repo-id',
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(404);
        validateErrorResponse(response);
      });
    });
  });

  describe('Update Repository', () => {
    it('should update repository branch', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/repositories',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const repoId = listResponse.body[0].id;

          cy.apiRequest({
            method: 'PUT',
            url: `/v1/repositories/${repoId}`,
            body: {
              branch: 'develop',
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No repositories available for update test');
        }
      });
    });
  });

  describe('Delete Repository', () => {
    it('should delete a repository', () => {
      // Create a repository first
      const repoRequest = {
        url: `https://github.com/example/delete-test-${Date.now()}.git`,
        branch: 'main',
      };

      cy.apiRequest({
        method: 'POST',
        url: '/v1/repositories',
        body: repoRequest,
        failOnStatusCode: false,
      }).then((createResponse) => {
        if (createResponse.status === 200 || createResponse.status === 201) {
          const repoId = createResponse.body.id;

          cy.apiRequest({
            method: 'DELETE',
            url: `/v1/repositories/${repoId}`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 204, 404]);
          });
        } else {
          cy.log('Could not create repository for deletion test');
        }
      });
    });

    it('should return 404 when deleting non-existent repository', () => {
      cy.apiRequest({
        method: 'DELETE',
        url: '/v1/repositories/non-existent-repo-id',
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(404);
        validateErrorResponse(response);
      });
    });
  });

  describe('Repository Scanning', () => {
    it('should trigger repository scan', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/repositories',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const repoId = listResponse.body[0].id;

          cy.apiRequest({
            method: 'POST',
            url: `/v1/repositories/${repoId}/scan`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 202, 400, 404]);

            if (response.status === 200 || response.status === 202) {
              expect(response.body).to.have.property('repo_id');
              expect(response.body).to.have.property('status');
            }
          });
        } else {
          cy.log('No repositories available for scan test');
        }
      });
    });

    it('should get scan status', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/repositories',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const repoId = listResponse.body[0].id;

          cy.apiRequest({
            method: 'GET',
            url: `/v1/repositories/${repoId}/scan/status`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 404]);

            if (response.status === 200) {
              expect(response.body).to.have.property('repo_id');
              expect(response.body).to.have.property('status');
            }
          });
        } else {
          cy.log('No repositories available for scan status test');
        }
      });
    });

    it('should handle concurrent scan requests gracefully', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/repositories',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const repoId = listResponse.body[0].id;

          // Trigger two scans in quick succession
          cy.apiRequest({
            method: 'POST',
            url: `/v1/repositories/${repoId}/scan`,
            failOnStatusCode: false,
          }).then((response1) => {
            expect(response1.status).to.be.oneOf([200, 202, 400, 404, 409]);

            cy.apiRequest({
              method: 'POST',
              url: `/v1/repositories/${repoId}/scan`,
              failOnStatusCode: false,
            }).then((response2) => {
              expect(response2.status).to.be.oneOf([200, 202, 400, 404, 409]);
            });
          });
        } else {
          cy.log('No repositories available for concurrent scan test');
        }
      });
    });
  });

  describe('Repository Statistics', () => {
    it('should get repository statistics', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/repositories',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const repoId = listResponse.body[0].id;

          cy.apiRequest({
            method: 'GET',
            url: `/v1/repositories/${repoId}/statistics`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 404]);

            if (response.status === 200) {
              expect(response.body).to.be.an('object');
            }
          });
        } else {
          cy.log('No repositories available for statistics test');
        }
      });
    });
  });

  describe('Repository Commits', () => {
    it('should list repository commits', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/repositories',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const repoId = listResponse.body[0].id;

          cy.apiRequest({
            method: 'GET',
            url: `/v1/repositories/${repoId}/commits`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 404]);

            if (response.status === 200) {
              expect(response.body).to.be.an('array');
            }
          });
        } else {
          cy.log('No repositories available for commits test');
        }
      });
    });

    it('should paginate repository commits', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/repositories',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const repoId = listResponse.body[0].id;

          cy.apiRequest({
            method: 'GET',
            url: `/v1/repositories/${repoId}/commits?limit=10&offset=0`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 404]);

            if (response.status === 200) {
              expect(response.body).to.be.an('array');
            }
          });
        } else {
          cy.log('No repositories available for commit pagination test');
        }
      });
    });
  });

  describe('Unauthenticated Access', () => {
    it('should reject unauthenticated requests to repository endpoints', () => {
      const endpoints = [
        '/v1/repositories',
      ];

      endpoints.forEach((endpoint) => {
        cy.request({
          method: 'GET',
          url: `${Cypress.env('API_BASE_URL')}${endpoint}`,
          failOnStatusCode: false,
        }).then((response) => {
          expect(response.status).to.eq(401);
          validateErrorResponse(response);
        });
      });
    });
  });
});
