/// <reference types="cypress" />
import '../../support/commands';

/**
 * Complete Adapter Lifecycle E2E Test
 *
 * This test covers the full lifecycle of an adapter from creation to deletion:
 * 1. Create and upload a dataset
 * 2. Start a training job with configured parameters
 * 3. Monitor training until completion
 * 4. Register the trained adapter
 * 5. Load the adapter into memory
 * 6. Run inference with the loaded adapter
 * 7. Unload and delete the adapter
 *
 * This test validates the complete happy path for adapter management.
 */
describe('Adapter Lifecycle E2E Test', () => {
  const testDatasetId = 'lifecycle-test-dataset';
  const testDatasetName = 'Lifecycle Test Dataset';
  const testJobId = 'lifecycle-test-job';
  const testAdapterId = 'lifecycle-test-adapter';
  const testAdapterName = 'lifecycle-test-adapter-v1';
  const testStackId = 'lifecycle-test-stack';
  const testModelId = 'qwen7b';

  beforeEach(() => {
    cy.login();
  });

  afterEach(() => {
    cy.cleanupTestData();
  });

  it('completes full adapter lifecycle from dataset upload to deletion', () => {
    // ========================================
    // STEP 1: Create and Upload Dataset
    // ========================================
    cy.log('Step 1: Creating and uploading dataset');

    // Mock dataset creation
    cy.intercept('POST', '**/v1/datasets', {
      statusCode: 201,
      body: {
        schema_version: 'v1',
        id: testDatasetId,
        name: testDatasetName,
        description: 'Test dataset for adapter lifecycle',
        file_count: 1,
        total_size_bytes: 2048000,
        format: 'jsonl',
        hash_b3: 'ds-hash-123',
        storage_path: `/datasets/${testDatasetId}`,
        validation_status: 'pending',
        trust_state: 'unknown',
        total_tokens: 25000,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
        tenant_id: 'default',
      },
    }).as('createDataset');

    // Mock dataset upload
    cy.intercept('POST', `**/v1/datasets/${testDatasetId}/upload`, {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        dataset_id: testDatasetId,
        status: 'uploaded',
        message: 'File uploaded successfully',
      },
    }).as('uploadDataset');

    // Mock dataset validation
    cy.intercept('POST', `**/v1/datasets/${testDatasetId}/validate`, {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        dataset_id: testDatasetId,
        is_valid: true,
        validation_status: 'valid',
        errors: [],
        trust_state: 'allowed',
        file_count: 1,
        total_tokens: 25000,
      },
    }).as('validateDataset');

    // Mock dataset get for polling
    cy.intercept('GET', `**/v1/datasets/${testDatasetId}`, {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        id: testDatasetId,
        name: testDatasetName,
        description: 'Test dataset for adapter lifecycle',
        file_count: 1,
        total_size_bytes: 2048000,
        format: 'jsonl',
        hash_b3: 'ds-hash-123',
        storage_path: `/datasets/${testDatasetId}`,
        validation_status: 'valid',
        trust_state: 'allowed',
        total_tokens: 25000,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
        tenant_id: 'default',
      },
    }).as('getDataset');

    // Mock datasets list
    cy.intercept('GET', '**/v1/datasets**', {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        datasets: [
          {
            id: testDatasetId,
            name: testDatasetName,
            file_count: 1,
            total_size_bytes: 2048000,
            validation_status: 'valid',
            trust_state: 'allowed',
            created_at: new Date().toISOString(),
          },
        ],
        total: 1,
      },
    }).as('listDatasets');

    // Navigate to datasets page
    cy.visit('/training/datasets');
    cy.get('[data-cy=datasets-page]', { timeout: 10000 }).should('be.visible');

    // Click upload dataset button
    cy.get('[data-cy=upload-dataset-btn], [data-cy=new-dataset-btn]').click();

    // Fill in dataset details
    cy.get('[data-cy=dataset-name-input]').type(testDatasetName);
    cy.get('[data-cy=dataset-description-input]').type('Test dataset for adapter lifecycle');

    // Upload file (simulate file selection - API will be mocked)
    // Note: Actual file upload is mocked via API intercepts
    cy.get('[data-cy=dataset-file-input]').then(($input) => {
      // Create a test file
      const blob = new Blob(
        ['{"text": "Sample training data", "label": "test"}'],
        { type: 'application/jsonl' }
      );
      const testFile = new File([blob], 'training_data.jsonl', {
        type: 'application/jsonl',
      });
      const dataTransfer = new DataTransfer();
      dataTransfer.items.add(testFile);
      const inputElement = $input[0] as HTMLInputElement;
      inputElement.files = dataTransfer.files;
      inputElement.dispatchEvent(new Event('change', { bubbles: true }));
    });

    // Submit dataset creation
    cy.get('[data-cy=create-dataset-btn]').click();
    cy.wait('@createDataset');

    // Wait for dataset to be processed/validated
    cy.wait('@validateDataset', { timeout: 15000 });

    // Verify dataset appears in list with valid status
    cy.get('[data-cy=dataset-card]', { timeout: 10000 })
      .contains(testDatasetName)
      .should('be.visible');

    cy.get('[data-cy=dataset-status]')
      .contains(/valid|ready/i)
      .should('be.visible');

    // ========================================
    // STEP 2: Start Training Job
    // ========================================
    cy.log('Step 2: Starting training job with configured parameters');

    // Mock model status
    cy.intercept('GET', '**/v1/models/status', {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        model_loaded: true,
        model_id: testModelId,
        backend: 'mlx',
      },
    }).as('modelStatus');

    // Mock training job creation
    cy.intercept('POST', '**/v1/training/jobs', {
      statusCode: 201,
      body: {
        schema_version: 'v1',
        job_id: testJobId,
        status: 'pending',
        adapter_name: testAdapterName,
        dataset_id: testDatasetId,
        rank: 8,
        alpha: 16,
        learning_rate: 0.0001,
        epochs: 3,
        batch_size: 4,
        created_at: new Date().toISOString(),
      },
    }).as('createTrainingJob');

    // Navigate to dataset detail or training page
    cy.visit(`/datasets/${testDatasetId}`);
    cy.wait('@getDataset');

    // Click start training button
    cy.get('[data-cy=dataset-start-training], [data-testid=dataset-start-training]')
      .should('not.be.disabled')
      .click();

    // Configure training parameters
    cy.get('[data-cy=training-job-form], [data-testid=quick-train-modal]').should('be.visible');

    // Set adapter name
    cy.get('[data-cy=adapter-name-input], [data-testid=quick-train-adapter-name]')
      .clear()
      .type(testAdapterName);

    // Open advanced settings if available
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=advanced-settings]').length > 0) {
        cy.get('[data-cy=advanced-settings]').click();

        // Set LoRA rank
        cy.get('[data-cy=rank-input]').clear().type('8');

        // Set LoRA alpha
        cy.get('[data-cy=alpha-input]').clear().type('16');

        // Set learning rate
        cy.get('[data-cy=learning-rate-input]').clear().type('0.0001');

        // Set epochs
        cy.get('[data-cy=epochs-input]').clear().type('3');

        // Set batch size
        cy.get('[data-cy=batch-size-input]').clear().type('4');
      }
    });

    // Start training
    cy.get('[data-cy=submit-training-job], [data-testid=quick-train-start]').click();
    cy.wait('@createTrainingJob');

    // ========================================
    // STEP 3: Monitor Training Progress
    // ========================================
    cy.log('Step 3: Monitoring training progress until completion');

    let pollCount = 0;
    let jobStatus = 'pending';

    // Mock training job status with progression
    cy.intercept('GET', `**/v1/training/jobs/${testJobId}`, (req) => {
      pollCount++;

      // Simulate progression: pending -> running -> completed
      if (pollCount >= 5) {
        jobStatus = 'completed';
      } else if (pollCount >= 2) {
        jobStatus = 'running';
      }

      const progress = jobStatus === 'completed' ? 100 : Math.min(pollCount * 20, 90);
      const currentEpoch = jobStatus === 'completed' ? 3 : Math.min(pollCount, 3);

      req.reply({
        statusCode: 200,
        body: {
          schema_version: 'v1',
          id: testJobId,
          status: jobStatus,
          adapter_name: testAdapterName,
          dataset_id: testDatasetId,
          progress,
          current_epoch: currentEpoch,
          total_epochs: 3,
          stack_id: jobStatus === 'completed' ? testStackId : null,
          adapter_id: jobStatus === 'completed' ? testAdapterId : null,
          rank: 8,
          alpha: 16,
          learning_rate: 0.0001,
          started_at: new Date().toISOString(),
          completed_at: jobStatus === 'completed' ? new Date().toISOString() : null,
          metrics: {
            loss: jobStatus === 'completed' ? 0.15 : 0.5 - (pollCount * 0.05),
            accuracy: jobStatus === 'completed' ? 0.95 : 0.5 + (pollCount * 0.05),
          },
        },
      });
    }).as('getTrainingJob');

    // Navigate to training jobs page to monitor
    cy.visit('/training');

    // Verify job appears in list
    cy.get('[data-cy=training-jobs-list]', { timeout: 10000 }).should('exist');
    cy.contains(testAdapterName).should('be.visible');

    // Monitor progress - should show running status
    cy.get('[data-cy=training-job-card]')
      .contains(testAdapterName)
      .parents('[data-cy=training-job-card]')
      .within(() => {
        // Should show progress bar while running
        cy.get('[data-cy=progress-bar], [data-cy=job-status]', { timeout: 5000 }).should('exist');
      });

    // Poll until completion
    cy.boundedPoll(
      () => {
        cy.visit('/training');
        return cy.get('body').then(($body) => {
          const completedJob = $body.find('[data-cy=job-status="completed"]').length > 0;
          if (!completedJob) {
            throw new Error('Job not completed yet');
          }
          return cy.wrap(true);
        });
      },
      { timeout: 30000, interval: 2000, description: 'Wait for training completion' }
    );

    // Verify training completed successfully
    cy.contains(testAdapterName)
      .parents('[data-cy=training-job-card]')
      .within(() => {
        cy.contains(/completed|success/i).should('be.visible');
      });

    // ========================================
    // STEP 4: Register Adapter
    // ========================================
    cy.log('Step 4: Registering the trained adapter');

    // Mock adapter registration
    cy.intercept('POST', '**/v1/adapters', {
      statusCode: 201,
      body: {
        schema_version: 'v1',
        id: testAdapterId,
        name: testAdapterName,
        model_id: testModelId,
        rank: 8,
        alpha: 16,
        training_job_id: testJobId,
        status: 'registered',
        is_loaded: false,
        created_at: new Date().toISOString(),
        hash_b3: 'adapter-hash-abc',
      },
    }).as('registerAdapter');

    // Mock adapters list
    cy.intercept('GET', '**/v1/adapters**', {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        adapters: [
          {
            id: testAdapterId,
            name: testAdapterName,
            model_id: testModelId,
            rank: 8,
            alpha: 16,
            status: 'registered',
            is_loaded: false,
            created_at: new Date().toISOString(),
          },
        ],
        total: 1,
      },
    }).as('listAdapters');

    // Mock adapter get
    cy.intercept('GET', `**/v1/adapters/${testAdapterId}`, {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        id: testAdapterId,
        name: testAdapterName,
        model_id: testModelId,
        rank: 8,
        alpha: 16,
        training_job_id: testJobId,
        status: 'registered',
        is_loaded: false,
        created_at: new Date().toISOString(),
        hash_b3: 'adapter-hash-abc',
      },
    }).as('getAdapter');

    // Navigate to adapters page
    cy.visit('/adapters');
    cy.wait('@listAdapters');

    // Check if adapter auto-registered, or manually register
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=adapter-card]').length === 0) {
        // Need to manually register
        cy.get('[data-cy=register-adapter-button]').click();
        cy.get('[data-cy=adapter-registration-dialog]').should('be.visible');

        // Fill registration form
        cy.get('[data-cy=adapter-name-input]').type(testAdapterName);
        cy.get('[data-cy=training-job-select]').click();
        cy.get(`[data-cy=job-option-${testJobId}]`).click();

        // Submit registration
        cy.get('[data-cy=register-adapter-submit]').click();
        cy.wait('@registerAdapter');
      }
    });

    // Verify adapter appears in list
    cy.get('[data-cy=adapter-card]', { timeout: 10000 })
      .contains(testAdapterName)
      .should('be.visible');

    // Track adapter for cleanup
    cy.trackResource('adapter', testAdapterId, `/v1/adapters/${testAdapterId}`);

    // ========================================
    // STEP 5: Load Adapter
    // ========================================
    cy.log('Step 5: Loading the adapter into memory');

    // Mock adapter load
    cy.intercept('POST', `**/v1/adapters/${testAdapterId}/load`, {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        id: testAdapterId,
        name: testAdapterName,
        status: 'loaded',
        is_loaded: true,
        message: 'Adapter loaded successfully',
      },
    }).as('loadAdapter');

    // Mock adapter status after loading
    cy.intercept('GET', `**/v1/adapters/${testAdapterId}`, {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        id: testAdapterId,
        name: testAdapterName,
        model_id: testModelId,
        rank: 8,
        alpha: 16,
        status: 'loaded',
        is_loaded: true,
        created_at: new Date().toISOString(),
        hash_b3: 'adapter-hash-abc',
      },
    }).as('getAdapterLoaded');

    // Find adapter card and load it
    cy.get('[data-cy=adapter-card]')
      .contains(testAdapterName)
      .parents('[data-cy=adapter-card]')
      .within(() => {
        // Open actions menu
        cy.get('[data-cy=adapter-actions], [data-cy=adapter-menu]').click();
      });

    // Click load adapter
    cy.get('[data-cy=load-adapter]').click();
    cy.wait('@loadAdapter');

    // Verify adapter status changed to loaded
    cy.get('[data-cy=adapter-card]')
      .contains(testAdapterName)
      .parents('[data-cy=adapter-card]')
      .within(() => {
        cy.get('[data-cy=adapter-status]')
          .contains(/loaded|active/i)
          .should('be.visible');
      });

    // ========================================
    // STEP 6: Run Inference
    // ========================================
    cy.log('Step 6: Running inference with the loaded adapter');

    // Mock inference response
    cy.intercept('POST', '**/v1/infer', {
      statusCode: 200,
      body: {
        schema_version: '1.0',
        id: 'run-lifecycle-test',
        text: 'This is a response from the lifecycle test adapter. The training was successful!',
        tokens_generated: 15,
        token_count: 15,
        latency_ms: 250,
        adapters_used: [testAdapterId],
        finish_reason: 'stop',
        run_receipt: {
          trace_id: 'trace-lifecycle-test',
          run_head_hash: 'head-hash-xyz',
          output_digest: 'out-digest-123',
          receipt_digest: 'rcpt-digest-456',
        },
        trace: {
          latency_ms: 250,
          router_decisions: [{ adapter: testAdapterId, score: 0.95 }],
          evidence_spans: [
            { text: 'Evidence from training data', relevance: 0.92 },
          ],
        },
      },
    }).as('runInference');

    // Navigate to inference page
    cy.visit('/inference');
    cy.get('[data-cy=inference-page]', { timeout: 10000 }).should('be.visible');

    // Select model
    cy.get('[data-cy=model-selector]').click();
    cy.get('[data-cy=model-option]').first().click();

    // Select the adapter
    cy.get('[data-cy=adapter-selector]').click();
    cy.get(`[data-cy=adapter-option-${testAdapterId}], [data-cy=adapter-option]`)
      .contains(testAdapterName)
      .click();

    // Enter prompt
    const testPrompt = 'Test prompt for lifecycle adapter verification';
    cy.get('[data-cy=prompt-input]').clear().type(testPrompt);

    // Run inference
    cy.get('[data-cy=run-inference-btn]').click();
    cy.wait('@runInference');

    // Verify response received
    cy.get('[data-cy=inference-output]', { timeout: 10000 })
      .should('be.visible')
      .and('contain.text', 'This is a response from the lifecycle test adapter');

    // Verify adapter was used
    cy.get('[data-cy=adapter-list], [data-cy=adapters-used]').within(() => {
      cy.contains(testAdapterId).should('be.visible');
    });

    // Verify receipt/proof information
    cy.get('[data-cy=proof-bar], [data-cy=receipt-info]').should('exist');

    // ========================================
    // STEP 7: Unload and Delete Adapter
    // ========================================
    cy.log('Step 7: Unloading and deleting the adapter');

    // Mock adapter unload
    cy.intercept('POST', `**/v1/adapters/${testAdapterId}/unload`, {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        id: testAdapterId,
        name: testAdapterName,
        status: 'registered',
        is_loaded: false,
        message: 'Adapter unloaded successfully',
      },
    }).as('unloadAdapter');

    // Mock adapter deletion
    cy.intercept('DELETE', `**/v1/adapters/${testAdapterId}`, {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        message: 'Adapter deleted successfully',
        id: testAdapterId,
      },
    }).as('deleteAdapter');

    // Navigate back to adapters page
    cy.visit('/adapters');

    // Unload adapter first
    cy.get('[data-cy=adapter-card]')
      .contains(testAdapterName)
      .parents('[data-cy=adapter-card]')
      .within(() => {
        cy.get('[data-cy=adapter-actions], [data-cy=adapter-menu]').click();
      });

    cy.get('[data-cy=unload-adapter]').click();
    cy.wait('@unloadAdapter');

    // Verify adapter is unloaded
    cy.get('[data-cy=adapter-card]')
      .contains(testAdapterName)
      .parents('[data-cy=adapter-card]')
      .within(() => {
        cy.get('[data-cy=adapter-status]')
          .contains(/registered|unloaded/i)
          .should('be.visible');
      });

    // Delete adapter
    cy.get('[data-cy=adapter-card]')
      .contains(testAdapterName)
      .parents('[data-cy=adapter-card]')
      .within(() => {
        cy.get('[data-cy=adapter-actions], [data-cy=adapter-menu]').click();
      });

    cy.get('[data-cy=delete-adapter]').click();

    // Confirm deletion if confirmation dialog appears
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=confirm-delete], [data-cy=delete-confirm]').length > 0) {
        cy.get('[data-cy=confirm-delete], [data-cy=delete-confirm]').click();
      }
    });

    cy.wait('@deleteAdapter');

    // Verify adapter removed from list
    cy.get('body').then(($body) => {
      if ($body.find('[data-cy=adapter-card]').length > 0) {
        cy.get('[data-cy=adapter-card]')
          .contains(testAdapterName)
          .should('not.exist');
      } else {
        // Empty state should be visible
        cy.get('[data-cy=empty-state]').should('be.visible');
      }
    });

    // Success toast should appear
    cy.get('[data-cy=success-message], [data-sonner-toast]')
      .contains(/deleted|removed/i)
      .should('be.visible');

    cy.log('✓ Complete adapter lifecycle test passed successfully!');
  });

  it('handles training failure gracefully', () => {
    cy.log('Testing training failure handling');

    // Mock a failed training job
    cy.intercept('POST', '**/v1/training/jobs', {
      statusCode: 201,
      body: {
        schema_version: 'v1',
        job_id: 'failed-job-id',
        status: 'pending',
        adapter_name: 'failed-adapter',
        created_at: new Date().toISOString(),
      },
    }).as('createFailedJob');

    cy.intercept('GET', '**/v1/training/jobs/failed-job-id', {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        id: 'failed-job-id',
        status: 'failed',
        adapter_name: 'failed-adapter',
        error_message: 'Training failed due to insufficient data',
        progress: 25,
        current_epoch: 1,
        total_epochs: 3,
        started_at: new Date().toISOString(),
        failed_at: new Date().toISOString(),
      },
    }).as('getFailedJob');

    cy.intercept('GET', '**/v1/datasets/test-dataset-fail', {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        id: 'test-dataset-fail',
        name: 'Test Failure Dataset',
        validation_status: 'valid',
        trust_state: 'allowed',
        created_at: new Date().toISOString(),
      },
    }).as('getFailDataset');

    cy.visit('/datasets/test-dataset-fail');
    cy.wait('@getFailDataset');

    // Start training
    cy.get('[data-cy=dataset-start-training], [data-testid=dataset-start-training]').click();
    cy.get('[data-cy=adapter-name-input], [data-testid=quick-train-adapter-name]')
      .clear()
      .type('failed-adapter');
    cy.get('[data-cy=submit-training-job], [data-testid=quick-train-start]').click();
    cy.wait('@createFailedJob');

    // Navigate to training page
    cy.visit('/training');

    // Verify error is displayed
    cy.contains('failed-adapter').should('be.visible');
    cy.get('[data-cy=training-job-card]')
      .contains('failed-adapter')
      .parents('[data-cy=training-job-card]')
      .within(() => {
        cy.contains(/failed|error/i).should('be.visible');
      });
  });

  it('prevents loading adapter without proper permissions', () => {
    cy.log('Testing permission-based access control');

    const restrictedAdapterId = 'restricted-adapter';

    cy.intercept('GET', '**/v1/adapters**', {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        adapters: [
          {
            id: restrictedAdapterId,
            name: 'Restricted Adapter',
            status: 'registered',
            is_loaded: false,
          },
        ],
        total: 1,
      },
    }).as('listRestrictedAdapters');

    // Mock load endpoint to return permission denied
    cy.intercept('POST', `**/v1/adapters/${restrictedAdapterId}/load`, {
      statusCode: 403,
      body: {
        error: 'Permission denied',
        code: 'FORBIDDEN',
        message: 'Insufficient permissions to load adapter',
      },
    }).as('loadRestricted');

    cy.visit('/adapters');
    cy.wait('@listRestrictedAdapters');

    // Try to load adapter
    cy.get('[data-cy=adapter-card]').first().within(() => {
      cy.get('[data-cy=adapter-actions], [data-cy=adapter-menu]').click();
    });

    cy.get('[data-cy=load-adapter]').click();
    cy.wait('@loadRestricted');

    // Verify error message is displayed
    cy.contains(/permission|forbidden|denied/i).should('be.visible');
  });
});
