import React, { createContext, useContext } from 'react';
import { Repository, TrainingTemplate } from '@/api/types';
import { DatasetSummary, WizardState } from './types';

export type SimpleDatasetMode = 'existing' | 'upload';

export type UploadStatus = 'idle' | 'creating' | 'validating';

export interface TrainingWizardContextValue {
  state: WizardState;
  updateState: (updates: Partial<WizardState>) => void;
  datasets: DatasetSummary[];
  repositories: Repository[];
  templates: TrainingTemplate[];
  simpleDatasetMode: SimpleDatasetMode;
  setSimpleDatasetMode: React.Dispatch<React.SetStateAction<SimpleDatasetMode>>;
  uploadFiles: File[];
  uploadError: string | null;
  createStatus: UploadStatus;
  validationResult: { status: string; errors?: string[]; warnings?: string[] } | null;
  datasetName: string;
  setDatasetName: React.Dispatch<React.SetStateAction<string>>;
  createdDatasetId: string | null;
  handleUploadFilesSelect: (files: FileList | null) => void;
  handleCreateAndValidateDataset: () => Promise<void>;
  handleOpenDatasetTools: (datasetId?: string | null) => void;
  dataSourceLocked: boolean;
}

const TrainingWizardContext = createContext<TrainingWizardContextValue | null>(null);

export const TrainingWizardProvider = TrainingWizardContext.Provider;

export const useTrainingWizardContext = (): TrainingWizardContextValue => {
  const context = useContext(TrainingWizardContext);
  if (!context) {
    throw new Error('TrainingWizardContext must be used within a provider');
  }
  return context;
};
