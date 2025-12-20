import { useCallback, useMemo, useState } from 'react';
import { apiClient } from '@/api/services';
import { logger, toError } from '@/utils/logger';

type OrchestratorStatus =
  | 'idle'
  | 'uploading'
  | 'processing'
  | 'creating_dataset'
  | 'done'
  | 'error';

type FileSource = {
  kind: 'file';
  file: File;
  name?: string;
  description?: string;
};

type DocumentSource = {
  kind: 'document';
  documentId: string;
  name?: string;
  description?: string;
};

type DocumentsSource = {
  kind: 'documents';
  documentIds: string[];
  name?: string;
  description?: string;
};

type CollectionSource = {
  kind: 'collection';
  collectionId: string;
  name?: string;
  description?: string;
};

export type TrainingDataSource =
  | FileSource
  | DocumentSource
  | DocumentsSource
  | CollectionSource;

export interface TrainingDataOrchestratorResult {
  datasetId: string;
  datasetName?: string;
  validationStatus?: string;
  documentId?: string;
}

export interface TrainingDataOrchestratorState {
  status: OrchestratorStatus;
  datasetId?: string | null;
  documentId?: string | null;
  error?: string | null;
  isBusy: boolean;
}

/**
 * Shared helper to convert a source (file upload, document, collection) into a dataset_id
 * via the canonical pipeline: upload → process → createDatasetFromDocuments.
 */
export function useTrainingDataOrchestrator() {
  const [status, setStatus] = useState<OrchestratorStatus>('idle');
  const [datasetId, setDatasetId] = useState<string | null>(null);
  const [documentId, setDocumentId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reset = useCallback(() => {
    setStatus('idle');
    setDatasetId(null);
    setDocumentId(null);
    setError(null);
  }, []);

  const orchestrate = useCallback(
    async (source: TrainingDataSource): Promise<TrainingDataOrchestratorResult> => {
      setError(null);
      setDatasetId(null);
      setDocumentId(null);

      try {
        let docIds: string[] = [];
        let effectiveDocumentId: string | undefined;
        const effectiveName =
          source.name?.trim() ||
          (source.kind === 'collection'
            ? `dataset-${source.collectionId}`
            : undefined);
        const effectiveDescription =
          source.description ||
          (source.kind === 'file'
            ? `Training dataset derived from ${source.file.name}`
            : undefined);

        if (source.kind === 'file') {
          setStatus('uploading');
          const uploaded = await apiClient.uploadDocument({
            file: source.file,
            name: source.name ?? source.file.name,
            description: effectiveDescription,
          });
          effectiveDocumentId = uploaded.document_id;
          setDocumentId(uploaded.document_id);

          setStatus('processing');
          await apiClient.processDocument(uploaded.document_id);
          docIds = [uploaded.document_id];
        } else if (source.kind === 'document') {
          effectiveDocumentId = source.documentId;
          setDocumentId(source.documentId);
          docIds = [source.documentId];
        } else if (source.kind === 'documents') {
          docIds = source.documentIds;
        }

        setStatus('creating_dataset');
        const dataset = await apiClient.createDatasetFromDocuments({
          // For collections, call the dedicated path
          ...(source.kind === 'collection'
            ? { collectionId: source.collectionId }
            : {
                document_ids: docIds,
                documentId: effectiveDocumentId,
              }),
          name: effectiveName,
          description: effectiveDescription ?? source.description,
        });

        setDatasetId(dataset.dataset_id);
        setStatus('done');

        return {
          datasetId: dataset.dataset_id,
          datasetName: (dataset as { name?: string }).name,
          validationStatus: (dataset as { validation_status?: string }).validation_status,
          documentId: effectiveDocumentId,
        };
      } catch (err) {
        const errorObj = toError(err);
        setStatus('error');
        setError(errorObj.message || 'Failed to prepare training data');
        logger.error(
          'Training data orchestration failed',
          { component: 'useTrainingDataOrchestrator' },
          errorObj
        );
        throw errorObj;
      }
    },
    []
  );

  const state: TrainingDataOrchestratorState = useMemo(
    () => ({
      status,
      datasetId,
      documentId,
      error,
      isBusy: status !== 'idle' && status !== 'done' && status !== 'error',
    }),
    [status, datasetId, documentId, error]
  );

  return {
    state,
    orchestrate,
    reset,
  };
}

