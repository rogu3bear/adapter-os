import React, { createContext, useContext } from 'react';
import { Repository, TrainingTemplate } from '@/api/types';
import { Document, Collection } from '@/api/document-types';
import { DatasetSummary, WizardState } from './types';

export type SimpleDatasetMode = 'existing' | 'upload' | 'document' | 'collection';

export type UploadStatus = 'idle' | 'creating' | 'validating';

export type ConversionStatus = 'idle' | 'converting' | 'done' | 'error';

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
  // Document/collection mode fields
  documents: Document[];
  collections: Collection[];
  selectedDocumentId: string | null;
  setSelectedDocumentId: React.Dispatch<React.SetStateAction<string | null>>;
  selectedCollectionId: string | null;
  setSelectedCollectionId: React.Dispatch<React.SetStateAction<string | null>>;
  conversionStatus: ConversionStatus;
  conversionError: string | null;
  handleConvertToDataset: () => Promise<void>;
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
