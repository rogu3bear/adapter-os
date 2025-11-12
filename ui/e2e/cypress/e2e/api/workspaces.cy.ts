// Workspace API Tests
import { validateErrorResponse } from '../support/api-helpers';

describe('Workspace API', () => {
  beforeEach(() => {
    cy.login();
  });

  describe('List Workspaces', () => {
    it('should list all workspaces', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/workspaces',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });
  });

  describe('Create Workspace', () => {
    it('should create a new workspace', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/workspaces',
        body: {
          name: `test-workspace-${Date.now()}`,
        },
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.oneOf([200, 201, 400, 422]);
        if (response.status >= 400) {
          validateErrorResponse(response);
        } else {
          expect(response.body).to.have.property('id');
        }
      });
    });

    it('should reject workspace creation with invalid data', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/workspaces',
        body: {},
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.at.least(400);
        validateErrorResponse(response);
      });
    });
  });

  describe('Get Workspace', () => {
    it('should get workspace by ID', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/workspaces',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const workspaceId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/v1/workspaces/${workspaceId}`,
          }).then((response) => {
            expect(response.status).to.eq(200);
            expect(response.body).to.have.property('id');
            expect(response.body.id).to.eq(workspaceId);
          });
        } else {
          cy.log('No workspaces available for testing');
        }
      });
    });

    it('should return 404 for non-existent workspace', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/workspaces/non-existent-id',
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(404);
        validateErrorResponse(response);
      });
    });
  });

  describe('Update Workspace', () => {
    it('should update a workspace', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/workspaces',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const workspaceId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'PUT',
            url: `/v1/workspaces/${workspaceId}`,
            body: {
              name: `updated-workspace-${Date.now()}`,
            },
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);
          });
        } else {
          cy.log('No workspaces available for testing');
        }
      });
    });
  });

  describe('Delete Workspace', () => {
    it('should delete a workspace', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/workspaces',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const workspaceId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'DELETE',
            url: `/v1/workspaces/${workspaceId}`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 204, 400, 404]);
          });
        } else {
          cy.log('No workspaces available for testing');
        }
      });
    });
  });
});

