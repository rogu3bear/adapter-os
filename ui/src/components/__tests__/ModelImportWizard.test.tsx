import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ModelImportWizard } from '@/components/ModelImportWizard';
import apiClient from '@/api/client';
import { toast } from 'sonner';

// Mock dependencies
jest.mock('@/api/client');
jest.mock('sonner');

const mockedApiClient = apiClient as jest.Mocked<typeof apiClient>;

describe('ModelImportWizard', () => {
  const onComplete = jest.fn();
  const onCancel = jest.fn();

  beforeEach(() => {
    // Reset mocks before each test
    jest.clearAllMocks();
  });

  it('renders the first step (Model Name) initially', () => {
    render(<ModelImportWizard onComplete={onComplete} onCancel={onCancel} />);
    expect(screen.getByText('Model Name')).toBeInTheDocument();
    expect(screen.getByPlaceholderText('e.g., qwen2.5-7b-instruct')).toBeInTheDocument();
  });

  it('validates that the model name is required', async () => {
    render(<ModelImportWizard onComplete={onComplete} onCancel={onCancel} />);
    fireEvent.click(screen.getByText('Next'));
    
    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith('Model name is required');
    });
    expect(onComplete).not.toHaveBeenCalled();
  });

  it('allows navigation through all steps with valid data', async () => {
    render(<ModelImportWizard onComplete={onComplete} onCancel={onCancel} />);

    // Step 1: Model Name
    fireEvent.change(screen.getByPlaceholderText('e.g., qwen2.5-7b-instruct'), {
      target: { value: 'test-model' },
    });
    fireEvent.click(screen.getByText('Next'));

    // Step 2: Model Weights
    await waitFor(() => {
      expect(screen.getByText('Model Weights')).toBeInTheDocument();
    });
    fireEvent.change(screen.getByPlaceholderText('/path/to/model/weights.safetensors'), {
      target: { value: '/test/weights.safetensors' },
    });
    fireEvent.click(screen.getByText('Next'));

    // Step 3: Configuration
    await waitFor(() => {
      expect(screen.getByText('Configuration')).toBeInTheDocument();
    });
    fireEvent.change(screen.getByPlaceholderText('/path/to/model/config.json'), {
      target: { value: '/test/config.json' },
    });
    fireEvent.change(screen.getByPlaceholderText('/path/to/model/tokenizer.json'), {
      target: { value: '/test/tokenizer.json' },
    });
    fireEvent.click(screen.getByText('Next'));

    // Step 4: Review
    await waitFor(() => {
      expect(screen.getByText('Review')).toBeInTheDocument();
    });
    expect(screen.getByText('test-model')).toBeInTheDocument();
    expect(screen.getByText('/test/weights.safetensors')).toBeInTheDocument();
  });

  it('submits the form and calls onComplete on the final step', async () => {
    mockedApiClient.importModel.mockResolvedValue({
      import_id: 'import-123',
      status: 'validating',
      message: 'Import started',
    });

    render(<ModelImportWizard onComplete={onComplete} onCancel={onCancel} />);

    // Fill form and navigate to the end
    fireEvent.change(screen.getByLabelText('Model Name'), { target: { value: 'final-model' } });
    fireEvent.click(screen.getByText('Next'));
    await waitFor(() => expect(screen.getByLabelText('Weights File Path')).toBeInTheDocument());
    fireEvent.change(screen.getByLabelText('Weights File Path'), { target: { value: 'model.safetensors' } });
    fireEvent.click(screen.getByText('Next'));
    await waitFor(() => expect(screen.getByLabelText('Config File Path')).toBeInTheDocument());
    fireEvent.change(screen.getByLabelText('Config File Path'), { target: { value: 'config.json' } });
    fireEvent.change(screen.getByLabelText('Tokenizer File Path'), { target: { value: 'tokenizer.json' } });
    fireEvent.click(screen.getByText('Next'));

    // Review and submit
    await waitFor(() => expect(screen.getByText('Import Model')).toBeInTheDocument());
    fireEvent.click(screen.getByText('Import Model'));

    await waitFor(() => {
      expect(mockedApiClient.importModel).toHaveBeenCalledWith({
        model_name: 'final-model',
        weights_path: 'model.safetensors',
        config_path: 'config.json',
        tokenizer_path: 'tokenizer.json',
        tokenizer_config_path: undefined,
        metadata: {},
      });
    });

    expect(toast.success).toHaveBeenCalledWith('Model import started: import-123');
    expect(onComplete).toHaveBeenCalledWith('import-123');
  });

  it('calls onCancel when the cancel button is clicked', () => {
    render(<ModelImportWizard onComplete={onComplete} onCancel={onCancel} />);
    fireEvent.click(screen.getByText('Cancel'));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});
