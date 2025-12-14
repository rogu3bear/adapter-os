/// <reference types="cypress" />
import '../../support/commands';

/**
 * E2E test for the 3-click training flow:
 * 1. Dataset Detail Page -> Click "Train"
 * 2. QuickTrainConfirmModal -> Click "Start Training"
 * 3. Training Complete Toast -> Click "Open Result Chat"
 *
 * Verifies the streamlined flow from validated dataset to result chat.
 */
describe('Quick Train Flow - 3 Click Training', () => {
  const mockDatasetId = 'quick-train-test-dataset';
  const mockJobId = 'quick-train-test-job';
  const mockStackId = 'quick-train-test-stack';
  const mockAdapterId = 'quick-train-test-adapter';

  beforeEach(() => {
    cy.login();

    // Mock dataset - validated and trusted
    cy.intercept('GET', `**/v1/datasets/${mockDatasetId}`, {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        id: mockDatasetId,
        name: 'Quick Train Test Dataset',
        description: 'A validated dataset for testing quick train flow',
        file_count: 25,
        total_size_bytes: 5 * 1024 * 1024, // 5 MB
        format: 'jsonl',
        hash_b3: 'abc123',
        storage_path: '/test/path',
        validation_status: 'valid',
        trust_state: 'allowed',
        total_tokens: 50000,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
        tenant_id: 'default',
      },
    }).as('getDataset');

    // Mock model status (for preflight)
    cy.intercept('GET', '**/v1/models/status', {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        model_loaded: true,
        model_id: 'qwen7b',
        backend: 'mlx',
      },
    }).as('getModelStatus');

    // Mock training job creation
    cy.intercept('POST', '**/v1/training/jobs', {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        job_id: mockJobId,
        status: 'pending',
        adapter_name: 'quick-train-test-dataset-adapter',
      },
    }).as('createTrainingJob');
  });

  it('completes training in 3 clicks from dataset detail', () => {
    // Mock job status - initially pending, then running, then completed
    let jobStatus = 'pending';
    let pollCount = 0;

    cy.intercept('GET', `**/v1/training/jobs/${mockJobId}`, (req) => {
      pollCount++;
      // Simulate progression: pending -> running -> completed
      if (pollCount >= 3) {
        jobStatus = 'completed';
      } else if (pollCount >= 1) {
        jobStatus = 'running';
      }

      req.reply({
        statusCode: 200,
        body: {
          schema_version: 'v1',
          id: mockJobId,
          status: jobStatus,
          adapter_name: 'quick-train-test-dataset-adapter',
          progress: jobStatus === 'completed' ? 100 : pollCount * 30,
          current_epoch: jobStatus === 'completed' ? 3 : pollCount,
          total_epochs: 3,
          stack_id: jobStatus === 'completed' ? mockStackId : null,
          adapter_id: jobStatus === 'completed' ? mockAdapterId : null,
          dataset_id: mockDatasetId,
          started_at: new Date().toISOString(),
          completed_at: jobStatus === 'completed' ? new Date().toISOString() : null,
        },
      });
    }).as('getTrainingJob');

    // Mock chat bootstrap for result chat
    cy.intercept('GET', `**/v1/training/jobs/${mockJobId}/chat_bootstrap`, {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        ready: true,
        stack_id: mockStackId,
        adapter_ids: [mockAdapterId],
        base_model: 'qwen7b',
        collection_id: null,
        suggested_chat_title: 'Chat with quick-train-test-dataset-adapter',
        training_job_id: mockJobId,
        status: 'completed',
        adapter_id: mockAdapterId,
        adapter_version_id: 'v1-abc123',
        dataset_id: mockDatasetId,
        dataset_version_id: 'dv-xyz789',
        dataset_name: 'Quick Train Test Dataset',
      },
    }).as('getChatBootstrap');

    // Step 1: Visit dataset detail page
    cy.visit(`/datasets/${mockDatasetId}`);
    cy.wait('@getDataset');

    // Verify dataset is displayed
    cy.contains('Quick Train Test Dataset').should('be.visible');

    // CLICK 1: Click Train button
    cy.get('[data-cy=dataset-start-training], [data-testid=dataset-start-training]')
      .should('not.be.disabled')
      .click();

    // Verify QuickTrainConfirmModal opens
    cy.get('[data-testid=quick-train-modal]').should('be.visible');

    // Verify preflight passed
    cy.get('[data-testid=quick-train-preflight-passed]').should('be.visible');

    // Verify adapter name is auto-generated
    cy.get('[data-testid=quick-train-adapter-name]')
      .should('have.value', 'quick-train-test-dataset-adapter');

    // CLICK 2: Click Start Training
    cy.get('[data-testid=quick-train-start]')
      .should('not.be.disabled')
      .click();

    // Verify training job was created
    cy.wait('@createTrainingJob');

    // Modal should close
    cy.get('[data-testid=quick-train-modal]').should('not.exist');

    // Wait for training to complete (polls job status)
    cy.wait('@getTrainingJob');

    // CLICK 3: Click "Open Result Chat" in completion toast
    // Note: The toast action navigates to the result chat page
    cy.get('[data-sonner-toast]')
      .contains('Open Result Chat', { timeout: 10000 })
      .click();

    // Verify we're on the result chat page
    cy.url().should('include', `/training/jobs/${mockJobId}/chat`);

    // Wait for chat bootstrap
    cy.wait('@getChatBootstrap');

    // Verify adapter and dataset chips are shown
    cy.contains('Adapter: quick-train-test-dataset-adapter').should('be.visible');
    cy.contains('Dataset: Quick Train Test Dataset').should('be.visible');
  });

  it('shows preflight errors for invalid datasets', () => {
    // Mock an invalid dataset
    cy.intercept('GET', `**/v1/datasets/invalid-dataset`, {
      statusCode: 200,
      body: {
        schema_version: 'v1',
        id: 'invalid-dataset',
        name: 'Invalid Dataset',
        description: null,
        file_count: 0, // No files - will fail preflight
        total_size_bytes: 0,
        format: 'jsonl',
        hash_b3: 'def456',
        storage_path: '/test/invalid',
        validation_status: 'invalid', // Not validated
        trust_state: 'blocked', // Trust blocked
        total_tokens: 0,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
        tenant_id: 'default',
      },
    }).as('getInvalidDataset');

    cy.visit('/datasets/invalid-dataset');
    cy.wait('@getInvalidDataset');

    // Train button should be disabled for invalid datasets
    // (canUseQuickTrain returns false, so button routes to wizard or is disabled)
    cy.get('[data-cy=dataset-start-training], [data-testid=dataset-start-training]')
      .should('be.disabled');
  });

  it('allows canceling quick train modal', () => {
    cy.visit(`/datasets/${mockDatasetId}`);
    cy.wait('@getDataset');

    // Open modal
    cy.get('[data-cy=dataset-start-training], [data-testid=dataset-start-training]').click();
    cy.get('[data-testid=quick-train-modal]').should('be.visible');

    // Click cancel
    cy.get('[data-testid=quick-train-cancel]').click();

    // Modal should close
    cy.get('[data-testid=quick-train-modal]').should('not.exist');

    // Still on dataset page
    cy.url().should('include', `/datasets/${mockDatasetId}`);
  });

  it('navigates to advanced wizard when requested', () => {
    cy.visit(`/datasets/${mockDatasetId}`);
    cy.wait('@getDataset');

    // Open modal
    cy.get('[data-cy=dataset-start-training], [data-testid=dataset-start-training]').click();
    cy.get('[data-testid=quick-train-modal]').should('be.visible');

    // Click "Advanced..." button
    cy.get('[data-testid=quick-train-advanced-btn]').click();

    // Should navigate to training wizard
    cy.url().should('include', '/training');
  });
});
