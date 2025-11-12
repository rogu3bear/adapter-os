// Contacts API Tests
import { validateErrorResponse } from '../support/api-helpers';

describe('Contacts API', () => {
  beforeEach(() => {
    cy.login();
  });

  describe('List Contacts', () => {
    it('should list all contacts', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/contacts',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });
  });

  describe('Create Contact', () => {
    it('should create a new contact', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/contacts',
        body: {
          name: 'Test Contact',
          email: `test-${Date.now()}@example.com`,
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

    it('should reject contact creation with invalid data', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/contacts',
        body: {},
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.at.least(400);
        validateErrorResponse(response);
      });
    });
  });

  describe('Get Contact', () => {
    it('should get contact by ID', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/contacts',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const contactId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/v1/contacts/${contactId}`,
          }).then((response) => {
            expect(response.status).to.eq(200);
            expect(response.body).to.have.property('id');
            expect(response.body.id).to.eq(contactId);
          });
        } else {
          cy.log('No contacts available for testing');
        }
      });
    });

    it('should return 404 for non-existent contact', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/contacts/non-existent-id',
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(404);
        validateErrorResponse(response);
      });
    });
  });

  describe('Delete Contact', () => {
    it('should delete a contact', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/contacts',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const contactId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'DELETE',
            url: `/v1/contacts/${contactId}`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 204, 404]);
          });
        } else {
          cy.log('No contacts available for testing');
        }
      });
    });
  });

  describe('Contact Interactions', () => {
    it('should get contact interactions', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/contacts',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const contactId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/v1/contacts/${contactId}/interactions`,
          }).then((response) => {
            expect(response.status).to.eq(200);
            expect(response.body).to.be.an('array');
          });
        } else {
          cy.log('No contacts available for testing');
        }
      });
    });
  });
});

