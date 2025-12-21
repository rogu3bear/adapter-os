/**
 * Component prop types for training-related UI components
 *
 * These types define the interfaces for components that manage training workflows,
 * datasets, metrics visualization, and adapter publishing.
 */

import type {
  TrainingJob,
  TrainingSession,
  TrainingMetrics,
  TrainingConfig,
  TrainingTemplate,
  Repository,
  Adapter,
} from '@/api/types';
import type { StartTrainingRequest } from '@/api/training-types';
import type { AttachMode, PublishAdapterResponse } from '@/api/adapter-types';

/**
 * Wizard state for training configuration
 */
export interface WizardState {
  /** Training category (code, framework, codebase, ephemeral) */
  category?: string;
  /** Adapter name */
  adapterName?: string;
  /** Adapter description */
  description?: string;
  /** Data source mode */
  dataSourceMode?: 'simple' | 'advanced';
  /** Dataset configuration */
  dataset?: {
    id?: string;
    versionId?: string;
    format?: string;
    [key: string]: unknown;
  };
  /** Training parameters */
  trainingParams?: {
    epochs?: number;
    learningRate?: number;
    batchSize?: number;
    [key: string]: unknown;
  };
  /** Packaging configuration */
  packaging?: {
    quantization?: string;
    compression?: boolean;
    [key: string]: unknown;
  };
  [key: string]: unknown;
}

/**
 * Dataset summary information
 */
export interface DatasetSummary {
  /** Dataset ID */
  id: string;
  /** Dataset name */
  name: string;
  /** Number of examples */
  exampleCount: number;
  /** Dataset format */
  format?: string;
  /** Dataset size in bytes */
  sizeBytes?: number;
  /** Creation timestamp */
  createdAt?: string;
}

/**
 * Props for TrainingWizard component
 * Multi-step wizard for creating and configuring training jobs
 */
export interface TrainingWizardProps {
  /** Callback invoked when training job is completed */
  onComplete: (trainingJobId: string) => void;
  /** Callback invoked when wizard is cancelled */
  onCancel: () => void;
  /** Initial dataset ID to pre-select */
  initialDatasetId?: string;
  /** When true, keep data source locked to the provided dataset */
  lockDatasetId?: boolean;
  /** When true, adjusts styling for standalone page rendering (not in dialog) */
  isStandalonePage?: boolean;
  /** When true, hides the simple/advanced mode toggle (defaults to simple) */
  hideSimpleModeToggle?: boolean;
}

/**
 * Props for TrainingMonitor component
 * Real-time monitoring of training job progress
 */
export interface TrainingMonitorProps {
  /** Training session ID to monitor */
  sessionId?: string;
  /** Training job ID to monitor */
  jobId?: string;
  /** Callback invoked when monitor is closed */
  onClose?: () => void;
}

/**
 * Props for MetricsComparison component
 * Side-by-side comparison of training metrics across jobs
 */
export interface MetricsComparisonProps {
  /** List of training jobs to compare */
  jobs: TrainingJob[];
  /** Historical metrics for each job (jobId -> metrics timeline) */
  metricsHistory?: Map<string, TrainingMetrics[]>;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Chart data point for metrics visualization
 */
export interface ChartDataPoint {
  /** Epoch number */
  epoch: number;
  /** Relative time in seconds from start */
  time?: number;
  /** Dynamic keys for each job's metrics */
  [key: string]: number | undefined;
}

/**
 * Props for PublishAdapterDialog component
 * Dialog for publishing an adapter version after training
 */
export interface PublishAdapterDialogProps {
  /** Whether the dialog is open */
  open: boolean;
  /** Callback to change open state */
  onOpenChange: (open: boolean) => void;
  /** Training job that produced the adapter */
  trainingJob: TrainingJob;
  /** Callback invoked when adapter is successfully published */
  onPublished?: (response: PublishAdapterResponse) => void;
}

/**
 * Preprocessing options for dataset configuration
 */
export interface PreprocessingOptions {
  /** Remove duplicate examples */
  removeDuplicates: boolean;
  /** Normalize whitespace in examples */
  normalizeWhitespace: boolean;
  /** Filter examples by token count */
  filterByTokenCount: boolean;
  /** Minimum token count (when filterByTokenCount is true) */
  minTokens?: number;
  /** Maximum token count (when filterByTokenCount is true) */
  maxTokens?: number;
  /** Remove empty files */
  removeEmptyFiles: boolean;
  /** Strip comments from code examples */
  stripComments: boolean;
}

/**
 * Tokenization settings for dataset configuration
 */
export interface TokenizationSettings {
  /** Tokenizer model to use */
  model?: string;
  /** Maximum sequence length */
  maxLength?: number;
  /** Whether to truncate long sequences */
  truncation: boolean;
  /** Whether to pad short sequences */
  padding: boolean;
}

/**
 * Dataset configuration data
 */
export interface DatasetConfigData {
  /** Dataset name */
  name: string;
  /** Dataset description */
  description: string;
  /** Dataset format (patches, jsonl, txt, custom) */
  format: 'patches' | 'jsonl' | 'txt' | 'custom';
  /** Preprocessing options */
  preprocessing: PreprocessingOptions;
  /** Tokenization settings (optional) */
  tokenization?: TokenizationSettings;
  /** Additional metadata */
  metadata?: Record<string, string>;
}

/**
 * Props for DatasetConfig component
 * Configuration interface for dataset preprocessing and tokenization
 */
export interface DatasetConfigProps {
  /** Initial configuration values */
  initialConfig?: Partial<DatasetConfigData>;
  /** Callback invoked when configuration changes */
  onChange: (config: DatasetConfigData) => void;
  /** Whether the configuration is disabled */
  disabled?: boolean;
}

/**
 * Props for DatasetSelector component
 * Interface for selecting datasets and versions
 */
export interface DatasetSelectorProps {
  /** Currently selected dataset ID */
  selectedDatasetId?: string;
  /** Currently selected version ID */
  selectedVersionId?: string;
  /** Callback invoked when dataset selection changes */
  onDatasetChange: (datasetId: string) => void;
  /** Callback invoked when version selection changes */
  onVersionChange: (versionId: string) => void;
  /** Whether the selector is disabled */
  disabled?: boolean;
  /** Optional tenant ID to filter datasets */
  tenantId?: string;
}

/**
 * Props for TrainingMetricsDisplay component
 * Displays current training metrics with visual indicators
 */
export interface TrainingMetricsDisplayProps {
  /** Current training metrics */
  metrics: TrainingMetrics;
  /** Whether training is currently active */
  isTraining: boolean;
  /** Optional comparison metrics */
  comparisonMetrics?: TrainingMetrics;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for DatasetPreview component
 * Preview of dataset examples before training
 */
export interface DatasetPreviewProps {
  /** Dataset ID to preview */
  datasetId: string;
  /** Version ID to preview (optional, uses latest if not specified) */
  versionId?: string;
  /** Maximum number of examples to show */
  maxExamples?: number;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for DatasetStats component
 * Statistical summary of a dataset
 */
export interface DatasetStatsProps {
  /** Dataset ID */
  datasetId: string;
  /** Version ID (optional) */
  versionId?: string;
  /** Whether to show detailed statistics */
  detailed?: boolean;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for QuickTrainConfirmModal component
 * Confirmation dialog for quick training
 */
export interface QuickTrainConfirmModalProps {
  /** Whether the modal is open */
  open: boolean;
  /** Callback to change open state */
  onOpenChange: (open: boolean) => void;
  /** Dataset to train on */
  dataset: {
    id: string;
    name: string;
    versionId?: string;
  };
  /** Callback invoked when training is confirmed */
  onConfirm: (config: Partial<TrainingConfig>) => void;
}

/**
 * Props for TrainingComparison component
 * Compare training jobs and their results
 */
export interface TrainingComparisonProps {
  /** List of job IDs to compare */
  jobIds: string[];
  /** Callback invoked when comparison is closed */
  onClose?: () => void;
}

/**
 * Props for DatasetVersionPicker component
 * Version selection interface for datasets
 */
export interface DatasetVersionPickerProps {
  /** Dataset ID */
  datasetId: string;
  /** Currently selected version ID */
  selectedVersionId?: string;
  /** Callback invoked when version changes */
  onVersionChange: (versionId: string) => void;
  /** Whether the picker is disabled */
  disabled?: boolean;
}

/**
 * Props for DatasetSplitConfig component
 * Configuration for train/validation/test splits
 */
export interface DatasetSplitConfigProps {
  /** Current split configuration */
  splitConfig: {
    train: number;
    validation: number;
    test: number;
  };
  /** Callback invoked when split configuration changes */
  onChange: (config: { train: number; validation: number; test: number }) => void;
  /** Whether the configuration is disabled */
  disabled?: boolean;
}
