import { AdapterCategory, AdapterScope } from '@/api/types';

export type DatasetSummary = {
  id: string;
  name: string;
  validation_status: string;
  file_count: number;
  total_size_bytes: number;
  validation_errors?: string;
};

export interface WizardState {
  currentStep?: number;
  category: AdapterCategory | null;
  name: string;
  description: string;
  scope: AdapterScope;
  dataSourceType: 'repository' | 'template' | 'custom' | 'directory' | 'dataset';
  repositoryId?: string;
  templateId?: string;
  customData?: string;
  datasetPath?: string;
  datasetId?: string;
  directoryRoot?: string;
  directoryPath?: string;
  language?: string;
  symbolTargets?: string[];
  frameworkId?: string;
  frameworkVersion?: string;
  apiPatterns?: string[];
  repoScope?: string;
  filePatterns?: string[];
  excludePatterns?: string[];
  ttlSeconds?: number;
  contextWindow?: number;
  rank: number;
  alpha: number;
  targets: string[];
  epochs: number;
  learningRate: number;
  batchSize: number;
  warmupSteps?: number;
  maxSeqLength?: number;
  packageAfter?: boolean;
  registerAfter?: boolean;
  createStack?: boolean;
  adaptersRoot?: string;
  adapterId?: string;
  tier?: string;
}
