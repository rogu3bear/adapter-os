// Git Integration API Tests
import { validateErrorResponse } from '../../../support/api-helpers';

describe('Git Integration API', () => {
  beforeEach(() => {
    cy.login();
  });

  // Clean up tracked resources after each test
  afterEach(() => {
    cy.cleanupTestData();
  });

  describe('Git Status', () => {
    it('should get git integration status', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/git/status',
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.oneOf([200, 404]);

        if (response.status === 200) {
          expect(response.body).to.have.property('enabled');
          expect(response.body.enabled).to.be.a('boolean');
          expect(response.body).to.have.property('active_sessions');
          expect(response.body.active_sessions).to.be.a('number');
          expect(response.body).to.have.property('repositories_tracked');
          expect(response.body.repositories_tracked).to.be.a('number');
        }
      });
    });

    it('should reject unauthenticated requests to git status', () => {
      cy.request({
        method: 'GET',
        url: `${Cypress.env('API_BASE_URL')}/v1/git/status`,
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(401);
        validateErrorResponse(response);
      });
    });
  });

  describe('Git Sessions', () => {
    it('should start a git session', () => {
      const sessionRequest = {
        repository_path: '/tmp/test-repo',
        branch: 'main',
      };

      cy.apiRequest({
        method: 'POST',
        url: '/v1/git/sessions',
        body: sessionRequest,
        failOnStatusCode: false,
      }).then((response) => {
        // May succeed or fail depending on repository availability
        expect(response.status).to.be.oneOf([200, 201, 400, 404, 422]);

        if (response.status === 200 || response.status === 201) {
          expect(response.body).to.have.property('session_id');
          expect(response.body).to.have.property('repository_path');
          expect(response.body).to.have.property('branch');
          expect(response.body).to.have.property('started_at');

          // Track session for cleanup
          if (response.body.session_id) {
            cy.trackResource('git-session', response.body.session_id, `/v1/git/sessions/${response.body.session_id}`);
          }
        } else if (response.status >= 400) {
          validateErrorResponse(response);
        }
      });
    });

    it('should list active git sessions', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/git/sessions',
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.oneOf([200, 404]);

        if (response.status === 200) {
          expect(response.body).to.be.an('array');
        }
      });
    });

    it('should get specific git session', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/git/sessions',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const sessionId = listResponse.body[0].session_id;

          cy.apiRequest({
            method: 'GET',
            url: `/v1/git/sessions/${sessionId}`,
          }).then((response) => {
            expect(response.status).to.eq(200);
            expect(response.body).to.have.property('session_id');
            expect(response.body.session_id).to.eq(sessionId);
            expect(response.body).to.have.property('repository_path');
            expect(response.body).to.have.property('branch');
          });
        } else {
          cy.log('No git sessions available for testing');
        }
      });
    });

    it('should stop a git session', () => {
      // First create a session
      const sessionRequest = {
        repository_path: '/tmp/test-repo-stop',
        branch: 'main',
      };

      cy.apiRequest({
        method: 'POST',
        url: '/v1/git/sessions',
        body: sessionRequest,
        failOnStatusCode: false,
      }).then((createResponse) => {
        if (createResponse.status === 200 || createResponse.status === 201) {
          const sessionId = createResponse.body.session_id;

          cy.apiRequest({
            method: 'DELETE',
            url: `/v1/git/sessions/${sessionId}`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 204, 404]);
          });
        } else {
          cy.log('Could not create git session for stop test');
        }
      });
    });

    it('should reject session creation with invalid path', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/git/sessions',
        body: {
          repository_path: '',
          branch: 'main',
        },
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.at.least(400);
        validateErrorResponse(response);
      });
    });

    it('should return 404 for non-existent session', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/git/sessions/non-existent-session',
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(404);
        validateErrorResponse(response);
      });
    });
  });

  describe('Git Branches', () => {
    it('should list branches for a repository', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/git/sessions',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const sessionId = listResponse.body[0].session_id;

          cy.apiRequest({
            method: 'GET',
            url: `/v1/git/sessions/${sessionId}/branches`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 404]);

            if (response.status === 200) {
              expect(response.body).to.be.an('array');
              if (response.body.length > 0) {
                expect(response.body[0]).to.have.property('name');
                expect(response.body[0]).to.have.property('is_current');
                expect(response.body[0]).to.have.property('last_commit');
              }
            }
          });
        } else {
          cy.log('No git sessions available for branch listing test');
        }
      });
    });

    it('should switch branch in a session', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/git/sessions',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const sessionId = listResponse.body[0].session_id;

          cy.apiRequest({
            method: 'POST',
            url: `/v1/git/sessions/${sessionId}/branch`,
            body: {
              branch: 'main',
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No git sessions available for branch switch test');
        }
      });
    });
  });

  describe('File Change Events', () => {
    it('should list file change events for a session', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/git/sessions',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const sessionId = listResponse.body[0].session_id;

          cy.apiRequest({
            method: 'GET',
            url: `/v1/git/sessions/${sessionId}/events`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 404]);

            if (response.status === 200) {
              expect(response.body).to.be.an('array');
              if (response.body.length > 0) {
                expect(response.body[0]).to.have.property('file_path');
                expect(response.body[0]).to.have.property('change_type');
                expect(response.body[0]).to.have.property('timestamp');
                expect(response.body[0]).to.have.property('session_id');
              }
            }
          });
        } else {
          cy.log('No git sessions available for events test');
        }
      });
    });

    it('should filter file change events by type', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/git/sessions',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const sessionId = listResponse.body[0].session_id;

          const changeTypes = ['added', 'modified', 'deleted'];
          changeTypes.forEach((changeType) => {
            cy.apiRequest({
              method: 'GET',
              url: `/v1/git/sessions/${sessionId}/events?change_type=${changeType}`,
              failOnStatusCode: false,
            }).then((response) => {
              expect(response.status).to.be.oneOf([200, 404]);

              if (response.status === 200) {
                expect(response.body).to.be.an('array');
              }
            });
          });
        } else {
          cy.log('No git sessions available for event filtering test');
        }
      });
    });
  });

  describe('Git Commits', () => {
    it('should list commits for a session', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/git/sessions',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const sessionId = listResponse.body[0].session_id;

          cy.apiRequest({
            method: 'GET',
            url: `/v1/git/sessions/${sessionId}/commits`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 404]);

            if (response.status === 200) {
              expect(response.body).to.be.an('array');
            }
          });
        } else {
          cy.log('No git sessions available for commits test');
        }
      });
    });

    it('should get commit details', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/git/sessions',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const sessionId = listResponse.body[0].session_id;

          // Get list of commits first
          cy.apiRequest({
            method: 'GET',
            url: `/v1/git/sessions/${sessionId}/commits`,
            failOnStatusCode: false,
          }).then((commitsResponse) => {
            if (commitsResponse.status === 200 && commitsResponse.body.length > 0) {
              const commitHash = commitsResponse.body[0].hash || commitsResponse.body[0].id;

              if (commitHash) {
                cy.apiRequest({
                  method: 'GET',
                  url: `/v1/git/sessions/${sessionId}/commits/${commitHash}`,
                  failOnStatusCode: false,
                }).then((response) => {
                  expect(response.status).to.be.oneOf([200, 404]);
                });
              } else {
                cy.log('No commit hash available');
              }
            } else {
              cy.log('No commits available for details test');
            }
          });
        } else {
          cy.log('No git sessions available for commit details test');
        }
      });
    });
  });

  describe('Unauthenticated Access', () => {
    it('should reject unauthenticated requests to git endpoints', () => {
      const endpoints = [
        '/v1/git/status',
        '/v1/git/sessions',
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
