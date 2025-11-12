// Authentication API Tests
import { getApiBaseUrl, getTestCredentials, validateLoginResponse, validateErrorResponse } from '../../support/api-helpers';

describe('Authentication API', () => {
  const apiBase = getApiBaseUrl();
  const credentials = getTestCredentials();

  // Note: No resource cleanup needed for auth tests (no resources created)

  describe('Login', () => {
    it('should login with valid credentials', () => {
      cy.request({
        method: 'POST',
        url: `${apiBase}/v1/auth/login`,
        body: credentials,
      }).then((response) => {
        validateLoginResponse(response);
        expect(response.body.token).to.be.a('string');
        expect(response.body.user_id).to.be.a('string');
        expect(response.body.role).to.be.a('string');
      });
    });

    it('should reject login with invalid email', () => {
      cy.request({
        method: 'POST',
        url: `${apiBase}/v1/auth/login`,
        body: {
          email: 'invalid@example.com',
          password: credentials.password,
        },
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(401);
        validateErrorResponse(response);
      });
    });

    it('should reject login with invalid password', () => {
      cy.request({
        method: 'POST',
        url: `${apiBase}/v1/auth/login`,
        body: {
          email: credentials.email,
          password: 'wrongpassword',
        },
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(401);
        validateErrorResponse(response);
      });
    });

    it('should reject login with missing fields', () => {
      cy.request({
        method: 'POST',
        url: `${apiBase}/v1/auth/login`,
        body: {
          email: credentials.email,
        },
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.at.least(400);
        validateErrorResponse(response);
      });
    });
  });

  describe('Authentication State', () => {
    beforeEach(() => {
      cy.login();
    });

    it('should get current user info from /v1/auth/me', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/auth/me',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.have.property('user_id');
        expect(response.body).to.have.property('email');
        expect(response.body).to.have.property('role');
      });
    });

    it('should reject /v1/auth/me without token', () => {
      cy.request({
        method: 'GET',
        url: `${apiBase}/v1/auth/me`,
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(401);
        validateErrorResponse(response);
      });
    });
  });

  describe('Token Refresh', () => {
    beforeEach(() => {
      cy.login();
    });

    it('should refresh token', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/auth/refresh',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.have.property('token');
        expect(response.body.token).to.be.a('string');
      });
    });
  });

  describe('Logout', () => {
    beforeEach(() => {
      cy.login();
    });

    it('should logout successfully', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/auth/logout',
      }).then((response) => {
        expect(response.status).to.eq(200);
      });
    });
  });

  describe('Session Management', () => {
    beforeEach(() => {
      cy.login();
    });

    it('should list active sessions', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/auth/sessions',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });

    it('should logout all sessions', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/auth/logout-all',
      }).then((response) => {
        expect(response.status).to.eq(200);
      });
    });
  });

  describe('Token Management', () => {
    beforeEach(() => {
      cy.login();
    });

    it('should get token metadata', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/auth/token',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.have.property('created_at');
      });
    });

    it('should rotate token', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/auth/token/rotate',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.have.property('token');
        expect(response.body.token).to.be.a('string');
      });
    });
  });

  describe('Profile Management', () => {
    beforeEach(() => {
      cy.login();
    });

    it('should get user profile', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/auth/me',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.have.property('user_id');
        expect(response.body).to.have.property('email');
      });
    });

    it('should update user profile', () => {
      cy.apiRequest({
        method: 'PUT',
        url: '/v1/auth/profile',
        body: {
          display_name: 'Test User Updated',
        },
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.have.property('user_id');
      });
    });
  });

  describe('Auth Configuration', () => {
    beforeEach(() => {
      cy.login();
    });

    it('should get auth configuration', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/auth/config',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.have.property('production_mode');
        expect(response.body).to.have.property('jwt_mode');
      });
    });
  });

  describe('Dev Bypass', () => {
    it('should support dev bypass endpoint if enabled', () => {
      cy.request({
        method: 'POST',
        url: `${apiBase}/v1/auth/dev-bypass`,
        failOnStatusCode: false,
      }).then((response) => {
        // May return 200 if enabled, 404/403 if disabled
        if (response.status === 200) {
          expect(response.body).to.have.property('token');
        } else {
          expect(response.status).to.be.oneOf([403, 404]);
        }
      });
    });
  });
});

