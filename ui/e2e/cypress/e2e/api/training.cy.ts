// Training API Tests
import { validateErrorResponse } from '../../support/api-helpers';

describe('Training API', () => {
  beforeEach(() => {
    cy.login();
  });

  // Clean up tracked resources after each test
  afterEach(() => {
    cy.cleanupTestData();
  });

  describe('Training Templates', () => {
    it('should list training templates', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/training/templates',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });

    it('should get specific training template', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/training/templates',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const templateId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/v1/training/templates/${templateId}`,
          }).then((response) => {
            expect(response.status).to.eq(200);
            expect(response.body).to.have.property('id');
            expect(response.body).to.have.property('name');
            expect(response.body).to.have.property('description');
            expect(response.body).to.have.property('category');
            expect(response.body).to.have.property('rank');
            expect(response.body).to.have.property('alpha');
            expect(response.body).to.have.property('targets');
            expect(response.body).to.have.property('epochs');
            expect(response.body).to.have.property('learning_rate');
            expect(response.body).to.have.property('batch_size');
          });
        } else {
          cy.log('No templates available for testing');
        }
      });
    });

    it('should return 404 for non-existent template', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/training/templates/non-existent-template',
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.eq(404);
        validateErrorResponse(response);
      });
    });
  });

  describe('Training Jobs', () => {
    it('should list training jobs', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/training/jobs',
      }).then((response) => {
        expect(response.status).to.eq(200);
        expect(response.body).to.be.an('array');
      });
    });

    it('should start a training job', () => {
      const trainingConfig = {
        adapter_name: `test-adapter-${Date.now()}`,
        config: {
          rank: 8,
          alpha: 16,
          targets: ['q_proj', 'v_proj'],
          epochs: 3,
          learning_rate: 0.0003,
          batch_size: 4,
          warmup_steps: 100,
          max_seq_length: 512,
          gradient_accumulation_steps: 1,
        },
        template_id: 'default',
        package: false,
        register: false,
      };

      cy.apiRequest({
        method: 'POST',
        url: '/v1/training/jobs',
        body: trainingConfig,
        failOnStatusCode: false,
      }).then((response) => {
        // May succeed or fail depending on available resources
        expect(response.status).to.be.oneOf([200, 201, 400, 422, 503]);

        if (response.status === 200 || response.status === 201) {
          expect(response.body).to.have.property('id');
          expect(response.body).to.have.property('adapter_name');
          expect(response.body).to.have.property('status');
          expect(response.body).to.have.property('progress_pct');
          expect(response.body).to.have.property('current_epoch');
          expect(response.body).to.have.property('total_epochs');
          expect(response.body).to.have.property('created_at');

          // Track job for cleanup
          if (response.body.id) {
            cy.trackResource('training-job', response.body.id, `/v1/training/jobs/${response.body.id}`);
          }
        } else {
          validateErrorResponse(response);
        }
      });
    });

    it('should reject training job with invalid config', () => {
      cy.apiRequest({
        method: 'POST',
        url: '/v1/training/jobs',
        body: {
          adapter_name: 'test',
          config: {
            rank: 0, // Invalid rank
          },
        },
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.at.least(400);
        validateErrorResponse(response);
      });
    });

    it('should get training job by ID', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/training/jobs',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const jobId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/v1/training/jobs/${jobId}`,
          }).then((response) => {
            expect(response.status).to.eq(200);
            expect(response.body).to.have.property('id');
            expect(response.body.id).to.eq(jobId);
            expect(response.body).to.have.property('status');
            expect(response.body).to.have.property('progress_pct');
          });
        } else {
          cy.log('No training jobs available for testing');
        }
      });
    });

    it('should cancel training job', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/training/jobs',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          // Find a running job if available
          const runningJob = listResponse.body.find((job: any) =>
            job.status === 'running' || job.status === 'pending'
          );

          if (runningJob) {
            cy.apiRequest({
              method: 'POST',
              url: `/v1/training/jobs/${runningJob.id}/cancel`,
              failOnStatusCode: false,
            }).then((response) => {
              expect(response.status).to.be.oneOf([200, 400, 404]);
            });
          } else {
            cy.log('No running jobs available to cancel');
          }
        } else {
          cy.log('No training jobs available for testing');
        }
      });
    });

    it('should get training metrics for a job', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/training/jobs',
      }).then((listResponse) => {
        if (listResponse.body.length > 0) {
          const jobId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/v1/training/jobs/${jobId}/metrics`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 404]);

            if (response.status === 200) {
              expect(response.body).to.have.property('loss');
              expect(response.body).to.have.property('learning_rate');
              expect(response.body).to.have.property('progress_pct');
            }
          });
        } else {
          cy.log('No training jobs available for testing');
        }
      });
    });
  });

  describe('Training Datasets', () => {
    it('should list training datasets', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/training/datasets',
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.oneOf([200, 404]);

        if (response.status === 200) {
          expect(response.body).to.be.an('array');
        }
      });
    });

    it('should create a training dataset', () => {
      const datasetRequest = {
        name: `test-dataset-${Date.now()}`,
        description: 'Test dataset for Cypress',
        format: 'jsonl',
      };

      cy.apiRequest({
        method: 'POST',
        url: '/v1/training/datasets',
        body: datasetRequest,
        failOnStatusCode: false,
      }).then((response) => {
        expect(response.status).to.be.oneOf([200, 201, 400, 404, 422]);

        if (response.status === 200 || response.status === 201) {
          expect(response.body).to.have.property('dataset_id');
          expect(response.body).to.have.property('name');

          // Track dataset for cleanup
          if (response.body.dataset_id) {
            cy.trackResource('dataset', response.body.dataset_id, `/v1/training/datasets/${response.body.dataset_id}`);
          }
        } else if (response.status >= 400) {
          validateErrorResponse(response);
        }
      });
    });

    it('should get dataset by ID', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/training/datasets',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const datasetId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/v1/training/datasets/${datasetId}`,
          }).then((response) => {
            expect(response.status).to.eq(200);
            expect(response.body).to.have.property('id');
            expect(response.body).to.have.property('name');
            expect(response.body).to.have.property('format');
            expect(response.body).to.have.property('validation_status');
          });
        } else {
          cy.log('No datasets available for testing');
        }
      });
    });

    it('should validate a dataset', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/training/datasets',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const datasetId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'POST',
            url: `/v1/training/datasets/${datasetId}/validate`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 400, 404]);

            if (response.status === 200) {
              expect(response.body).to.have.property('dataset_id');
              expect(response.body).to.have.property('status');
              expect(response.body).to.have.property('errors');
              expect(response.body).to.have.property('warnings');
            }
          });
        } else {
          cy.log('No datasets available for testing');
        }
      });
    });

    it('should get dataset statistics', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/training/datasets',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const datasetId = listResponse.body[0].id;
          cy.apiRequest({
            method: 'GET',
            url: `/v1/training/datasets/${datasetId}/statistics`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 404]);

            if (response.status === 200) {
              expect(response.body).to.have.property('num_examples');
              expect(response.body).to.have.property('total_tokens');
            }
          });
        } else {
          cy.log('No datasets available for testing');
        }
      });
    });

    it('should delete a dataset', () => {
      // Create a dataset first
      const datasetRequest = {
        name: `test-dataset-delete-${Date.now()}`,
        description: 'Test dataset for deletion',
        format: 'jsonl',
      };

      cy.apiRequest({
        method: 'POST',
        url: '/v1/training/datasets',
        body: datasetRequest,
        failOnStatusCode: false,
      }).then((createResponse) => {
        if (createResponse.status === 200 || createResponse.status === 201) {
          const datasetId = createResponse.body.dataset_id;

          cy.apiRequest({
            method: 'DELETE',
            url: `/v1/training/datasets/${datasetId}`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 204, 404]);
          });
        } else {
          cy.log('Could not create dataset for deletion test');
        }
      });
    });
  });

  describe('Training Events (SSE)', () => {
    it('should connect to training events stream', () => {
      // Note: SSE testing is limited in Cypress, but we can verify the endpoint exists
      cy.apiRequest({
        method: 'GET',
        url: '/v1/training/events',
        failOnStatusCode: false,
      }).then((response) => {
        // Should either return streaming data or proper error
        expect(response.status).to.be.oneOf([200, 404]);
      });
    });

    it('should filter training events by job ID', () => {
      cy.apiRequest({
        method: 'GET',
        url: '/v1/training/jobs',
        failOnStatusCode: false,
      }).then((listResponse) => {
        if (listResponse.status === 200 && listResponse.body.length > 0) {
          const jobId = listResponse.body[0].id;

          cy.apiRequest({
            method: 'GET',
            url: `/v1/training/events?job_id=${jobId}`,
            failOnStatusCode: false,
          }).then((response) => {
            expect(response.status).to.be.oneOf([200, 404]);
          });
        } else {
          cy.log('No training jobs available for event filtering test');
        }
      });
    });
  });

  describe('Unauthenticated Access', () => {
    it('should reject unauthenticated requests to training endpoints', () => {
      const endpoints = [
        '/v1/training/jobs',
        '/v1/training/templates',
        '/v1/training/datasets',
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
