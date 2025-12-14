
// 【ui/src/components/TrainingWizard.tsx§1-981】 - Add density controls and breadcrumbs

import React, { useState, useEffect, useMemo, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { Wizard, WizardStep } from './ui/wizard';
import { Button } from './ui/button';
import { Alert, AlertDescription } from './ui/alert';
import { ErrorRecovery } from './ui/error-recovery';
import { Label } from './ui/label';
import { Switch } from './ui/switch';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from './ui/dialog';
import { RotateCcw, Sparkles } from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '@/api/client';
import { logger, toError } from '@/utils/logger';
import { DensityProvider, useDensity } from '@/contexts/DensityContext';
import { BreadcrumbNavigation } from './BreadcrumbNavigation';
import { useWizardPersistence } from '@/hooks/persistence/useWizardPersistence';
import { TrainingConfigSchema, formatValidationError } from '@/schemas';
import { TERMS } from '@/constants/terminology';
import { TrainingConfig, TrainingTemplate, Repository } from '@/api/types';
import { StartTrainingRequest } from '@/api/training-types';
import { ZodError } from 'zod';
import { FILE_VALIDATION } from './TrainingWizard/constants';
import { TrainingWizardProvider, SimpleDatasetMode, ConversionStatus } from './TrainingWizard/context';
import { useDocuments } from '@/hooks/documents';
import { useCollections } from '@/hooks/api/useCollectionsApi';
import { useTrainingDataOrchestrator } from '@/hooks/training';
import { CategoryStep } from './TrainingWizard/steps/CategoryStep';
import { BasicInfoStep } from './TrainingWizard/steps/BasicInfoStep';
import { SimpleDatasetStep } from './TrainingWizard/steps/SimpleDatasetStep';
import { SimpleTrainingParamsStep } from './TrainingWizard/steps/SimpleTrainingParamsStep';
import { DataSourceStep } from './TrainingWizard/steps/DataSourceStep';
import { CategoryConfigStep } from './TrainingWizard/steps/CategoryConfigStep';
import { TrainingParamsStep } from './TrainingWizard/steps/TrainingParamsStep';
import { PackagingStep } from './TrainingWizard/steps/PackagingStep';
import { ReviewStep } from './TrainingWizard/steps/ReviewStep';
import { WizardState, DatasetSummary } from './TrainingWizard/types';

/**
 * Maps backend error codes to user-friendly messages
 * Error codes match those returned by backend training handlers
 */
function getTrainingErrorMessage(code: string, fallback: string): string {
  const messages: Record<string, string> = {
    'VALIDATION_ERROR': 'Please fix the highlighted configuration issues and try again.',
    'NOT_FOUND': 'The dataset or template was deleted. Please reselect.',
    'TENANT_ISOLATION_ERROR': 'This dataset belongs to a different tenant.',
    'POLICY_VIOLATION': 'Policy requirements not met. Check dataset validation status.',
    'TRAINING_CAPACITY_LIMIT': 'System is at capacity. Try again later or reduce batch size.',
    'MEMORY_PRESSURE_CRITICAL': 'Memory pressure too high. Reduce batch size or wait.',
    'TRAINING_ERROR': 'Training failed. Check the Training Jobs page for details.',
    'INTERNAL_ERROR': 'An internal error occurred. Please try again later.',
    'DATABASE_ERROR': 'Database error. Please try again later.',
  };
  return messages[code] || fallback;
}

interface TrainingWizardProps {
  onComplete: (trainingJobId: string) => void;
  onCancel: () => void;
  initialDatasetId?: string;
  /** When true, keep data source locked to the provided dataset */
  lockDatasetId?: boolean;
  /** When true, adjusts styling for standalone page rendering (not in dialog) */
  isStandalonePage?: boolean;
  /** When true, hides the simple/advanced mode toggle (defaults to simple) */
  hideSimpleModeToggle?: boolean;
}

// Inner component that uses density context
function TrainingWizardInner({ onComplete, onCancel, initialDatasetId, lockDatasetId, hideSimpleModeToggle = false }: TrainingWizardProps & { hideSimpleModeToggle?: boolean }): JSX.Element {
  const { spacing, textSizes } = useDensity();
  const navigate = useNavigate();
  const [isLoading, setIsLoading] = useState(false);
  const [repositories, setRepositories] = useState<Repository[]>([]);
  const [templates, setTemplates] = useState<TrainingTemplate[]>([]);
  const [datasets, setDatasets] = useState<DatasetSummary[]>([]);
  const [wizardError, setWizardError] = useState<Error | null>(null);
  const [validationError, setValidationError] = useState<string | null>(null);
  const [showResumeDialog, setShowResumeDialog] = useState(false);
  const [savedState, setSavedState] = useState<WizardState | null>(null);
  const [simpleMode, setSimpleMode] = useState(true); // Default to simple mode for MVP
  const [simpleDatasetMode, setSimpleDatasetMode] = useState<SimpleDatasetMode>('existing');
  const [uploadFiles, setUploadFiles] = useState<File[]>([]);
  const [uploadError, setUploadError] = useState<string | null>(null);
  const [createStatus, setCreateStatus] = useState<'idle' | 'creating' | 'validating'>('idle');
  const [createdDatasetId, setCreatedDatasetId] = useState<string | null>(null);
  const [validationResult, setValidationResult] = useState<{ status: string; errors?: string[]; warnings?: string[] } | null>(null);
  const [datasetName, setDatasetName] = useState('');
  const dataSourceLocked = Boolean(initialDatasetId && lockDatasetId);

  // Document/collection mode state
  const { data: documentsData } = useDocuments();
  const { data: collectionsData } = useCollections();
  const documents = documentsData ?? [];
  const collections = collectionsData ?? [];
  const [selectedDocumentId, setSelectedDocumentId] = useState<string | null>(null);
  const [selectedCollectionId, setSelectedCollectionId] = useState<string | null>(null);
  const [conversionStatus, setConversionStatus] = useState<ConversionStatus>('idle');
  const [conversionError, setConversionError] = useState<string | null>(null);
  const { orchestrate: orchestrateTrainingData } = useTrainingDataOrchestrator();

  const initialState: WizardState = {
    currentStep: 0,
    category: null,
    name: '',
    description: '',
    scope: 'global',
    dataSourceType: initialDatasetId ? 'dataset' : 'template',
    datasetId: initialDatasetId,
    rank: 8,
    alpha: 16,
    targets: ['q_proj', 'v_proj'],
    epochs: 3,
    learningRate: 3e-4,
    batchSize: 4,
    packageAfter: true,
    registerAfter: true,
    createStack: true,
    adaptersRoot: './adapters',
    tier: 'warm',
  };

  const {
    state,
    setState: setPersistedState,
    clearState: clearPersistedState,
    hasSavedState,
    loadSavedState,
  } = useWizardPersistence<WizardState & Record<string, unknown>>({
    storageKey: 'training-wizard',
    initialState: initialState as WizardState & Record<string, unknown>,
    onSavedStateDetected: (saved) => {
      setSavedState(saved as WizardState);
      setShowResumeDialog(true);
    },
  });

  const [currentStep, setCurrentStep] = useState(state.currentStep || 0);

  // Sync currentStep with state persistence
  useEffect(() => {
    setPersistedState({ currentStep });
  }, [currentStep, setPersistedState]);

  useEffect(() => {
    // Load repositories and templates
    const loadData = async () => {
      try {

        setWizardError(null);

        const [reposData, templatesData, datasetsData] = await Promise.all([
          apiClient.listRepositories(),
          apiClient.listTrainingTemplates(),
          apiClient.listDatasets().catch(() => ({ datasets: [] })), // Gracefully handle if datasets endpoint fails
        ]);
        setRepositories(reposData);
        setTemplates(templatesData);
        const mappedDatasets: DatasetSummary[] = (datasetsData.datasets || []).map((d) => ({
          id: d.id,
          name: d.name,
          validation_status: d.validation_status || 'draft',
          file_count: d.file_count || 0,
          total_size_bytes: d.total_size_bytes || 0,
          validation_errors: Array.isArray(d.validation_errors) ? d.validation_errors.join('; ') : d.validation_errors,
        }));
        setDatasets(mappedDatasets);
      } catch (error) {

        const err = error instanceof Error ? error : new Error('Failed to load repositories and templates');
        setWizardError(err);
        logger.error('Failed to preload training wizard data: repositories and templates could not be loaded', {
          component: 'TrainingWizard',
          operation: 'loadData',
          errorType: 'data_loading_failure',
          details: 'Failed to fetch repositories list and training templates'
        }, toError(error));

        toast.error('Failed to load repositories and templates');
      }
    };
    loadData();
  }, []);


  const handleResume = () => {
    // loadSavedState already updates the persisted state
    const restoredState = loadSavedState();
    if (restoredState && restoredState.currentStep !== undefined) {
      setCurrentStep(restoredState.currentStep);
    }
    setShowResumeDialog(false);
  };

  const handleStartFresh = () => {
    clearPersistedState();
    // Reset to initial state
    setPersistedState(initialState as WizardState & Record<string, unknown>);
    setCurrentStep(0);
    setSimpleDatasetMode('existing');
    setUploadFiles([]);
    setValidationResult(null);
    setCreatedDatasetId(null);
    setShowResumeDialog(false);
  };

  // Wrapper to reset conversion state when switching modes
  const handleSimpleDatasetModeChange = useCallback((mode: SimpleDatasetMode | ((prev: SimpleDatasetMode) => SimpleDatasetMode)) => {
    setSimpleDatasetMode(mode);
    // Reset conversion state to prevent stale data between mode switches
    setConversionStatus('idle');
    setConversionError(null);
    setCreatedDatasetId(null);
    // Reset selections when switching away from document/collection modes
    if (typeof mode === 'string') {
      if (mode !== 'document') {
        setSelectedDocumentId(null);
      }
      if (mode !== 'collection') {
        setSelectedCollectionId(null);
      }
    }
  }, []);

  const updateState = useCallback((updates: Partial<WizardState>) => {
    setPersistedState(updates);
  }, [setPersistedState]);

  const validateUploadFile = useCallback((file: File): string | null => {
    if (file.size > FILE_VALIDATION.maxSize) {
      return `File ${file.name} exceeds ${FILE_VALIDATION.maxSize / (1024 * 1024)}MB`;
    }
    const extension = '.' + file.name.split('.').pop()?.toLowerCase();
    if (!FILE_VALIDATION.allowedExtensions.includes(extension)) {
      return `Unsupported type ${extension}`;
    }
    return null;
  }, []);

  const handleUploadFilesSelect = useCallback((files: FileList | null) => {
    if (!files) return;
    const valid: File[] = [];
    const errors: string[] = [];
    Array.from(files).forEach((file) => {
      const err = validateUploadFile(file);
      if (err) {
        errors.push(err);
        return;
      }
      const duplicate = uploadFiles.some(f => f.name === file.name && f.size === file.size);
      if (!duplicate) {
        valid.push(file);
      }
    });
    if (errors.length > 0) {
      setUploadError(errors.join('\n'));
    } else {
      setUploadError(null);
    }
    if (valid.length > 0) {
      setUploadFiles(prev => [...prev, ...valid]);
    }
  }, [uploadFiles, validateUploadFile]);

  const handleCreateAndValidateDataset = useCallback(async () => {
    if (uploadFiles.length === 0) {
      setUploadError('Add at least one file to continue');
      return;
    }
    setUploadError(null);
    setCreateStatus('creating');
    setValidationResult(null);
    try {
      const nameToUse = (datasetName || state.name || '').trim() || `dataset-${Date.now()}`;
      const response = await apiClient.createDataset({
        name: nameToUse,
        source_type: 'uploaded_files',
        files: uploadFiles,
      });
      const newDatasetId = response.dataset.id;
      setCreatedDatasetId(newDatasetId);
      updateState({
        datasetId: newDatasetId,
        dataSourceType: 'dataset',
        name: state.name || response.dataset.name || nameToUse,
      });
      setDatasets(prev => [
        {
          id: newDatasetId,
          name: response.dataset.name || nameToUse,
          validation_status: response.dataset.validation_status || 'draft',
          file_count: response.dataset.file_count || 0,
          total_size_bytes: response.dataset.total_size_bytes || 0,
          validation_errors: response.dataset.validation_errors
        },
        ...prev.filter(d => d.id !== newDatasetId),
      ]);

      setCreateStatus('validating');
      const result = await apiClient.validateDataset(newDatasetId);
      setValidationResult(result);
      setDatasets(prev => prev.map(d => d.id === newDatasetId ? { ...d, validation_status: result.status } : d));

      if (result.status === 'valid') {
        toast.success('Dataset uploaded and validated');
      } else {
        toast.error('Dataset validation reported issues');
      }
    } catch (error) {
      const err = toError(error);
      setUploadError(err.message);
      logger.error('Wizard dataset upload failed', { component: 'TrainingWizard' }, err);
    } finally {
      setCreateStatus('idle');
    }
  }, [uploadFiles, datasetName, state.name, updateState]);

  const handleOpenDatasetTools = useCallback((datasetId?: string | null) => {
    if (!datasetId) return;
    navigate(`/training/datasets/${datasetId}`, { state: { focus: 'validation' } });
  }, [navigate]);

  // Convert document or collection to JSONL dataset
  const handleConvertToDataset = useCallback(async () => {
    if (!selectedDocumentId && !selectedCollectionId) {
      setConversionError('Please select a document or collection first');
      return;
    }

    setConversionStatus('converting');
    setConversionError(null);

    try {
      // Determine name based on source type
      let sourceName = '';
      if (selectedDocumentId) {
        const doc = documents.find((d: { document_id: string; name?: string }) => d.document_id === selectedDocumentId);
        sourceName = doc?.name || 'Untitled Document';
      } else if (selectedCollectionId) {
        const col = collections.find(c => c.collection_id === selectedCollectionId);
        sourceName = col?.name || 'Untitled Collection';
      }

      const orchestrationResult = await orchestrateTrainingData(
        selectedDocumentId
          ? {
              kind: 'document' as const,
              documentId: selectedDocumentId,
              name: `Training from doc: ${sourceName}`,
              description: `Training dataset derived from document ${sourceName}`,
            }
          : {
              kind: 'collection' as const,
              collectionId: selectedCollectionId as string,
              name: `Training from collection: ${sourceName}`,
              description: `Training dataset derived from collection ${sourceName}`,
            }
      );

      const newDatasetId = orchestrationResult.datasetId;
      const datasetName =
        orchestrationResult.datasetName ||
        (selectedDocumentId
          ? `Training from doc: ${sourceName}`
          : `Training from collection: ${sourceName}`);
      const validationStatus = orchestrationResult.validationStatus || 'valid';

      // Update state with new dataset
      updateState({
        datasetId: newDatasetId,
        dataSourceType: 'dataset',
        name: state.name || sourceName,
      });

      // Add to datasets list for the dropdown
      setDatasets(prev => [
        {
          id: newDatasetId,
          name: datasetName,
          validation_status: validationStatus,
          file_count: 1,
          total_size_bytes: 0,
        },
        ...prev.filter(d => d.id !== newDatasetId),
      ]);

      setCreatedDatasetId(newDatasetId);
      setConversionStatus('done');
      toast.success('Dataset created from documents');
    } catch (error) {
      const err = toError(error);
      setConversionError(err.message);
      setConversionStatus('error');
      logger.error('Failed to convert documents to dataset', { component: 'TrainingWizard' }, err);
      toast.error('Failed to create dataset: ' + err.message);
    }
  }, [selectedDocumentId, selectedCollectionId, documents, collections, state.name, updateState]);

  const trainingWizardContextValue = useMemo(() => ({
    state,
    updateState,
    datasets,
    repositories,
    templates,
    simpleDatasetMode,
    setSimpleDatasetMode: handleSimpleDatasetModeChange,
    uploadFiles,
    uploadError,
    createStatus,
    validationResult,
    datasetName,
    setDatasetName,
    createdDatasetId,
    handleUploadFilesSelect,
    handleCreateAndValidateDataset,
    handleOpenDatasetTools,
    dataSourceLocked,
    // Document/collection mode
    documents,
    collections,
    selectedDocumentId,
    setSelectedDocumentId,
    selectedCollectionId,
    setSelectedCollectionId,
    conversionStatus,
    conversionError,
    handleConvertToDataset,
  }), [
    state,
    updateState,
    datasets,
    repositories,
    templates,
    simpleDatasetMode,
    handleSimpleDatasetModeChange,
    uploadFiles,
    uploadError,
    createStatus,
    validationResult,
    datasetName,
    setDatasetName,
    createdDatasetId,
    handleUploadFilesSelect,
    handleCreateAndValidateDataset,
    handleOpenDatasetTools,
    dataSourceLocked,
    documents,
    collections,
    selectedDocumentId,
    setSelectedDocumentId,
    selectedCollectionId,
    setSelectedCollectionId,
    conversionStatus,
    conversionError,
    handleConvertToDataset,
  ]);

    // Step components lifted into separate files (see TrainingWizard/steps)

  const handleComplete = async () => {
    setWizardError(null);
    setValidationError(null);
    setIsLoading(true);
    try {
      // For simple mode, ensure all required fields are set
      let stateToValidate = { ...state };
      if (simpleMode) {
        if (!stateToValidate.datasetId) {
          setValidationError(TERMS.datasetRequired);
          setIsLoading(false);
          return;
        }

        // Enforce collection validation status
        const selectedDataset = datasets.find(d => d.id === stateToValidate.datasetId);
        if (selectedDataset && selectedDataset.validation_status !== 'valid') {
          setValidationError(
            `Collection "${selectedDataset.name}" must be validated before training. ` +
            `Current status: ${selectedDataset.validation_status}. ` +
            `Please validate from the Document Collections page.`
          );
          setIsLoading(false);
          return;
        }
        // Set all required defaults for simple mode
        if (!stateToValidate.name || stateToValidate.name.trim() === '') {
          stateToValidate.name = `adapter-${Date.now()}`;
        }
        if (!stateToValidate.category) {
          stateToValidate.category = 'codebase';
        }
        if (!stateToValidate.scope) {
          stateToValidate.scope = 'tenant';
        }
        if (stateToValidate.targets.length === 0) {
          stateToValidate.targets = ['q_proj', 'v_proj'];
        }
        if (stateToValidate.packageAfter === undefined) {
          stateToValidate.packageAfter = true;
        }
        if (stateToValidate.registerAfter === undefined) {
          stateToValidate.registerAfter = true;
        }
        if (stateToValidate.createStack === undefined) {
          stateToValidate.createStack = true;
        }
        if (!stateToValidate.adaptersRoot) {
          stateToValidate.adaptersRoot = './adapters';
        }
        if (!stateToValidate.tier) {
          stateToValidate.tier = 'warm';
        }
        // Update persisted state with all defaults
        updateState(stateToValidate);
      }

      const selectedDataset =
        stateToValidate.dataSourceType === 'dataset' && stateToValidate.datasetId
          ? datasets.find((d) => d.id === stateToValidate.datasetId)
          : undefined;

      if (stateToValidate.dataSourceType === 'dataset') {
        if (!stateToValidate.datasetId) {
          setValidationError(TERMS.datasetRequired);
          setIsLoading(false);
          return;
        }
        if (selectedDataset && selectedDataset.validation_status !== 'valid') {
          setValidationError(
            `Collection ${selectedDataset.id} is not validated (status: ${selectedDataset.validation_status}). Please run validation first.`
          );
          setIsLoading(false);
          return;
        }
      }

      // Validate form data against schema
      const validationResult = await TrainingConfigSchema.parseAsync({
        name: stateToValidate.name,
        description: stateToValidate.description,
        category: stateToValidate.category,
        scope: stateToValidate.scope,
        dataSourceType: stateToValidate.dataSourceType,
        templateId: stateToValidate.templateId,
        repositoryId: stateToValidate.repositoryId,
        customData: stateToValidate.customData,
        datasetPath: stateToValidate.datasetPath,
        directoryRoot: stateToValidate.directoryRoot,
        directoryPath: stateToValidate.directoryPath,
        language: stateToValidate.language,
        symbolTargets: stateToValidate.symbolTargets,
        frameworkId: stateToValidate.frameworkId,
        frameworkVersion: stateToValidate.frameworkVersion,
        apiPatterns: stateToValidate.apiPatterns,
        repoScope: stateToValidate.repoScope,
        filePatterns: stateToValidate.filePatterns,
        excludePatterns: stateToValidate.excludePatterns,
        ttlSeconds: stateToValidate.ttlSeconds,
        contextWindow: stateToValidate.contextWindow,
        rank: stateToValidate.rank,
        alpha: stateToValidate.alpha,
        epochs: stateToValidate.epochs,
        learningRate: stateToValidate.learningRate,
        batchSize: stateToValidate.batchSize,
        targets: stateToValidate.targets,
        warmupSteps: stateToValidate.warmupSteps,
        maxSeqLength: stateToValidate.maxSeqLength,
        packageAfter: stateToValidate.packageAfter,
        registerAfter: stateToValidate.registerAfter,
        createStack: stateToValidate.createStack,
        adaptersRoot: stateToValidate.adaptersRoot,
        adapterId: stateToValidate.adapterId,
        tier: stateToValidate.tier,
      });

      // Build training config
      const trainingConfig: TrainingConfig = {
        rank: stateToValidate.rank,
        alpha: stateToValidate.alpha,
        targets: stateToValidate.targets,
        epochs: stateToValidate.epochs,
        learning_rate: stateToValidate.learningRate,
        batch_size: stateToValidate.batchSize,
        warmup_steps: stateToValidate.warmupSteps,
        max_seq_length: stateToValidate.maxSeqLength,
      };

      // Start training - build request matching backend StartTrainingRequest
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const trainingRequest: any = {
        adapter_name: stateToValidate.name,
        config: trainingConfig,

        // Category & metadata
        category: stateToValidate.category || 'codebase',
        description: stateToValidate.description || undefined,

        // Post-training actions (wrapped in post_actions object)
        post_actions: {
          package: stateToValidate.packageAfter ?? true,
          register: stateToValidate.registerAfter ?? true,
          create_stack: stateToValidate.createStack ?? true, // Default true: create stack but NOT set as default
          tier: stateToValidate.tier || 'warm',
          adapters_root: stateToValidate.adaptersRoot || undefined,
        },
      };

      switch (stateToValidate.category) {
        case 'code':
          trainingRequest.language = stateToValidate.language;
          if (stateToValidate.symbolTargets && stateToValidate.symbolTargets.length > 0) {
            trainingRequest.symbol_targets = stateToValidate.symbolTargets;
          }
          break;
        case 'framework':
          trainingRequest.framework_id = stateToValidate.frameworkId;
          trainingRequest.framework_version = stateToValidate.frameworkVersion;
          if (stateToValidate.apiPatterns && stateToValidate.apiPatterns.length > 0) {
            trainingRequest.api_patterns = stateToValidate.apiPatterns;
          }
          break;
        case 'codebase':
          trainingRequest.repo_scope = stateToValidate.repoScope;
          if (stateToValidate.filePatterns && stateToValidate.filePatterns.length > 0) {
            trainingRequest.file_patterns = stateToValidate.filePatterns;
          }
          if (stateToValidate.excludePatterns && stateToValidate.excludePatterns.length > 0) {
            trainingRequest.exclude_patterns = stateToValidate.excludePatterns;
          }
          break;
        case 'ephemeral':
          if (stateToValidate.ttlSeconds) {
            trainingRequest.ttl_seconds = stateToValidate.ttlSeconds;
          }
          if (stateToValidate.contextWindow) {
            trainingRequest.context_window = stateToValidate.contextWindow;
          }
          break;
      }

      // Add data source based on type
      if (stateToValidate.dataSourceType === 'template' && stateToValidate.templateId) {
        trainingRequest.template_id = stateToValidate.templateId;
      } else if (stateToValidate.dataSourceType === 'repository' && stateToValidate.repositoryId) {
        trainingRequest.repo_id = stateToValidate.repositoryId;
      } else if (stateToValidate.dataSourceType === 'dataset' && stateToValidate.datasetId) {
        // Dataset-based training
        trainingRequest.dataset_id = stateToValidate.datasetId;
      } else if (stateToValidate.dataSourceType === 'directory') {
        // Directory-based training
        trainingRequest.directory_root = stateToValidate.directoryRoot;
        trainingRequest.directory_path = stateToValidate.directoryPath || '.';
      } else if (stateToValidate.dataSourceType === 'custom') {
        // For custom, dataset_path is included
      }

      if (stateToValidate.datasetPath) {
        trainingRequest.dataset_path = stateToValidate.datasetPath;
      }

      const job = await apiClient.startTraining(trainingRequest);

      // Success - training started, clear persisted state
      clearPersistedState();
      toast.success(`Training job ${job.id} started successfully!`, {
        action: {
          label: 'View Progress',
          onClick: () => navigate(`/training/jobs/${job.id}`),
        },
      });
      onComplete(job.id);
    } catch (error) {
      if (error instanceof ZodError) {
        // Format Zod validation errors
        const validationResult = formatValidationError(error);
        const firstError = validationResult.errors[0];
        setValidationError(firstError?.message || 'Validation failed');
        logger.warn('Training wizard validation failed', {
          component: 'TrainingWizard',
          operation: 'validateForm',
          errorCount: validationResult.errors.length,
        });
      } else {
        // Extract error code from API response if available
        const apiError = error as { code?: string; response?: { data?: { code?: string } }; message?: string };
        const errorCode = apiError?.code || apiError?.response?.data?.code || 'UNKNOWN_ERROR';
        const errorMessage = getTrainingErrorMessage(errorCode, apiError?.message || 'Failed to start training');

        const err = new Error(errorMessage);
        setWizardError(err);

        // Handle specific error codes
        if (errorCode === 'TRAINING_CAPACITY_LIMIT' || errorCode === 'MEMORY_PRESSURE_CRITICAL') {
          toast.error(errorMessage, { duration: 8000 });
        } else {
          toast.error(errorMessage);
        }

        logger.error('Training job start failed', {
          component: 'TrainingWizard',
          operation: 'startTraining',
          adapterName: state.name,
          errorCode,
        }, toError(error));
      }
    } finally {
      setIsLoading(false);
    }
  };

  // Simple mode steps: Collection → Rank/Alpha → Review
  const simpleModeSteps: WizardStep[] = [
    {
      id: 'dataset',
      title: TERMS.selectDataset,
      description: 'Choose your training data',
      component: <SimpleDatasetStep />,
      validate: () => {
        setValidationError(null);
        if (simpleDatasetMode === 'existing') {
          if (!state.datasetId?.trim()) {
            setValidationError(TERMS.datasetRequired);
            return false;
          }
          const selectedDataset = datasets.find(d => d.id === state.datasetId);
          if (selectedDataset && selectedDataset.validation_status !== 'valid') {
            setValidationError(`Collection "${selectedDataset.name}" must be validated before training. Current status: ${selectedDataset.validation_status}`);
            return false;
          }
          return true;
        }

        // Upload path
        const selectedStatus =
          validationResult?.status ||
          datasets.find(d => d.id === state.datasetId)?.validation_status;

        if (!state.datasetId || !createdDatasetId) {
          setValidationError('Upload and validate documents before continuing.');
          return false;
        }
        if (selectedStatus !== 'valid') {
          setValidationError('Uploaded collection must be valid before training.');
          return false;
        }
        return true;
      },
    },
    {
      id: 'training-params',
      title: 'Training Parameters',
      description: 'Set rank, alpha, and epochs',
      component: <SimpleTrainingParamsStep />,
      validate: () => {
        setValidationError(null);
        if (state.rank < 1) {
          setValidationError('Rank must be at least 1');
          return false;
        }
        if (state.alpha < 1) {
          setValidationError('Alpha must be at least 1');
          return false;
        }
        if (state.epochs < 1) {
          setValidationError('Epochs must be at least 1');
          return false;
        }
        return true;
      },
    },
    {
      id: 'review',
      title: 'Review & Start',
      description: 'Confirm and start training',
      component: <ReviewStep />,
    },
  ];

  // Full mode steps: All 7 steps
  const fullModeSteps: WizardStep[] = [
    {
      id: 'category',
      title: 'Category',
      description: 'Select adapter type',
      component: <CategoryStep />,
      validate: () => {
        setValidationError(null);
        if (!state.category) {
          setValidationError('Please select an adapter category');
          return false;
        }
        return true;
      },
    },
    {
      id: 'basic-info',
      title: 'Basic Info',
      description: 'Name and scope',
      component: <BasicInfoStep />,
      validate: () => {
        setValidationError(null);
        if (!state.name.trim()) {
          setValidationError('Adapter name is required');
          return false;
        }
        return true;
      },
    },
    {
      id: 'data-source',
      title: 'Data Source',
      description: 'Select training data',
      component: <DataSourceStep />,
      validate: () => {
        setValidationError(null);
        if (state.dataSourceType === 'template' && !state.templateId) {
          setValidationError('Please select a template');
          return false;
        }
        if (state.dataSourceType === 'repository' && !state.repositoryId) {
          setValidationError('Please select a repository');
          return false;
        }
        if (state.dataSourceType === 'directory' && !state.directoryRoot?.trim()) {
          setValidationError('Please provide a directory root path for directory-based training');
          return false;
        }
        if (state.dataSourceType === 'dataset' && !state.datasetId?.trim()) {
          setValidationError('Please select a dataset for dataset-based training');
          return false;
        }
        if (state.dataSourceType === 'dataset' && state.datasetId) {
          const selectedDataset = datasets.find(d => d.id === state.datasetId);
          if (selectedDataset && selectedDataset.validation_status !== 'valid') {
            setValidationError(`Dataset "${selectedDataset.name}" must be validated before training. Current status: ${selectedDataset.validation_status}`);
            return false;
          }
        }
        if (state.dataSourceType === 'custom' && !state.datasetPath?.trim()) {
          setValidationError('For custom training, please provide a dataset_path pointing to a training JSON file');
          return false;
        }
        return true;
      },
    },
    {
      id: 'category-config',
      title: 'Configuration',
      description: 'Category-specific settings',
      component: <CategoryConfigStep />,
    },
    {
      id: 'training-params',
      title: 'Training Parameters',
      description: 'Set training speed and style',
      component: <TrainingParamsStep />,
      validate: () => {
        setValidationError(null);
        if (state.targets.length === 0) {
          setValidationError('Please select at least one LoRA target module');
          return false;
        }
        return true;
      },
    },
    {
      id: 'packaging',
      title: 'Packaging & Registration',
      description: 'Artifacts and registry',
      component: <PackagingStep />,
    },
    {
      id: 'review',
      title: 'Review',
      description: 'Confirm and start',
      component: <ReviewStep />,
    },
  ];

  const steps = simpleMode ? simpleModeSteps : fullModeSteps;

  return (
    <TrainingWizardProvider value={trainingWizardContextValue}>
      <div className={spacing.sectionGap}>
      {/* Resume Dialog */}
      <Dialog open={showResumeDialog} onOpenChange={setShowResumeDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <RotateCcw className="h-5 w-5" />
              Resume Previous Session?
            </DialogTitle>
            <DialogDescription>
              We found a saved training configuration from a previous session. Would you like to resume where you left off?
            </DialogDescription>
          </DialogHeader>
          {savedState && (
            <div className="space-y-2 text-sm">
              <div className="flex items-center gap-2">
                <span className="text-muted-foreground">Adapter:</span>
                <span className="font-medium">{(savedState as WizardState).name || 'Untitled'}</span>
              </div>
              {(savedState as WizardState).category && (
                <div className="flex items-center gap-2">
                  <span className="text-muted-foreground">Category:</span>
                  <span className="font-medium capitalize">{(savedState as WizardState).category}</span>
                </div>
              )}
              {(savedState as WizardState)?.currentStep !== undefined && (
                <div className="flex items-center gap-2">
                  <span className="text-muted-foreground">Progress:</span>
                  <span className="font-medium">Step {((savedState as WizardState)?.currentStep ?? 0) + 1} of 7</span>
                </div>
              )}
            </div>
          )}
          <DialogFooter>
            <Button variant="outline" onClick={handleStartFresh}>
              Start Fresh
            </Button>
            <Button onClick={handleResume}>
              Resume
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <BreadcrumbNavigation />
      <div className="flex justify-between items-center mb-4">
        <h2 className={textSizes.title}>Training Wizard</h2>
        <div className="flex items-center gap-4">
          {!hideSimpleModeToggle && (
            <div className="flex items-center gap-2">
              <Switch
                id="simple-mode"
                checked={simpleMode}
                onCheckedChange={(checked) => {
                  setSimpleMode(checked);
                  // Reset to first step when switching modes
                  setCurrentStep(0);
                  // Reset state defaults for simple mode
                  if (checked) {
                    updateState({
                      dataSourceType: 'dataset',
                      packageAfter: true,
                      registerAfter: true,
                      createStack: true,
                      adaptersRoot: './adapters',
                      tier: 'warm',
                      targets: ['q_proj', 'v_proj'], // Default targets for simple mode
                    });
                  }
                }}
              />
              <Label htmlFor="simple-mode" className="flex items-center gap-2 cursor-pointer">
                <Sparkles className="h-4 w-4" />
                <span className="text-sm">Simple Mode</span>
              </Label>
            </div>
          )}
          {hasSavedState && !showResumeDialog && (
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                const saved = loadSavedState();
                if (saved && saved.currentStep !== undefined) {
                  setCurrentStep(saved.currentStep);
                }
              }}
              className="text-xs"
            >
              <RotateCcw className="h-3 w-3 mr-1" />
              Load Saved
            </Button>
          )}
        </div>
      </div>
      {simpleMode && !hideSimpleModeToggle && (
        <Alert className="mb-4">
          <Sparkles className="h-4 w-4" />
          <AlertDescription>
            Simple mode streamlines the training process to just 3 steps: {TERMS.selectDataset} → Configure Parameters → Start Training.
            Toggle off for advanced options like repositories, templates, and custom configurations.
          </AlertDescription>
        </Alert>
      )}

      {/* Error Recovery */}
      {wizardError && (
        <ErrorRecovery
          error={wizardError.message}
          onRetry={() => {
            setWizardError(null);
            setCurrentStep(0);
          }}
        />
      )}
      {validationError && (
        <ErrorRecovery
          error={validationError}
          onRetry={() => setValidationError(null)}
        />
      )}

      <Wizard
        title="Training Wizard"
        steps={steps}
        currentStep={currentStep}
        onStepChange={setCurrentStep}
        onComplete={handleComplete}
        onCancel={onCancel}
        completeButtonText="Start Training"
        isLoading={isLoading}
      />
    </div>
  </TrainingWizardProvider>
);
}

// Outer component with DensityProvider
export function TrainingWizard({ onComplete, onCancel, initialDatasetId, lockDatasetId = false, isStandalonePage = false, hideSimpleModeToggle = false }: TrainingWizardProps) {
  // When rendered as a standalone page, the page component provides DensityProvider
  // When rendered in a dialog, we need to provide it here
  if (isStandalonePage) {
    return (
      <TrainingWizardInner
        onComplete={onComplete}
        onCancel={onCancel}
        initialDatasetId={initialDatasetId}
        lockDatasetId={lockDatasetId}
        hideSimpleModeToggle={hideSimpleModeToggle}
      />
    );
  }

  return (
    <DensityProvider pageKey="training-wizard">
      <TrainingWizardInner
        onComplete={onComplete}
        onCancel={onCancel}
        initialDatasetId={initialDatasetId}
        lockDatasetId={lockDatasetId}
        hideSimpleModeToggle={hideSimpleModeToggle}
      />
    </DensityProvider>
  );
}
