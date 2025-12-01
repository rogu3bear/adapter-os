import React, { useState, useEffect } from 'react';
import { Wizard, WizardStep } from '@/components/ui/wizard';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from './ui/dialog';
import { Progress } from '@/components/ui/progress';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { Upload, FileCheck, Settings, CheckCircle, RotateCcw } from 'lucide-react';
import apiClient from '@/api/client';
import { ImportModelRequest } from '@/api/types';
import { useWizardPersistence } from '@/hooks/useWizardPersistence';
import { useProgressOperation } from '@/hooks/useProgressOperation';
import { useCancellableOperation } from '@/hooks/useCancellableOperation';

interface ModelImportWizardProps {
  onComplete: (importId: string) => void;
  onCancel: () => void;
  tenantId?: string;
}

interface WizardState {
  currentStep?: number;
  modelName: string;
  weightsPath: string;
  configPath: string;
  tokenizerPath: string;
  tokenizerConfigPath: string;
  metadata: Record<string, unknown>;
}

export function ModelImportWizard({ onComplete, onCancel, tenantId }: ModelImportWizardProps) {
  const [isLoading, setIsLoading] = useState(false);
  const [wizardError, setWizardError] = useState<Error | null>(null);
  const [validationError, setValidationError] = useState<string | null>(null);
  const [showResumeDialog, setShowResumeDialog] = useState(false);
  const [savedState, setSavedState] = useState<WizardState | null>(null);

  // Progress tracking for model import
  const { start: startModelImport } = useProgressOperation();

  // Cancellation support for model import
  const { start: startCancellableImport, cancel: cancelImport } = useCancellableOperation();

  const initialState: WizardState = {
    currentStep: 0,
    modelName: '',
    weightsPath: '',
    configPath: '',
    tokenizerPath: '',
    tokenizerConfigPath: '',
    metadata: {},
  };

  const {
    state,
    setState: setPersistedState,
    clearState: clearPersistedState,
    hasSavedState,
    loadSavedState,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- wizard persistence needs flexible state type
  } = useWizardPersistence<WizardState & Record<string, any>>({
    storageKey: 'model-import-wizard',
    initialState,
    onSavedStateDetected: (saved) => {
      setSavedState(saved);
      setShowResumeDialog(true);
    },
  });

  const [currentStep, setCurrentStep] = useState(state.currentStep || 0);

  // Sync currentStep with state persistence
  useEffect(() => {
    setPersistedState({ currentStep });
  }, [currentStep, setPersistedState]);

  const handleResume = () => {
    const restoredState = loadSavedState();
    if (restoredState && restoredState.currentStep !== undefined) {
      setCurrentStep(restoredState.currentStep);
    }
    setShowResumeDialog(false);
  };

  const handleStartFresh = () => {
    clearPersistedState();
    setPersistedState(initialState);
    setCurrentStep(0);
    setShowResumeDialog(false);
  };

  // Step 1: Model Name
  const ModelNameStep = () => (
    <div className="space-y-4">
      <div>
        <Label htmlFor="modelName">Model Name</Label>
        <Input
          id="modelName"
          placeholder="e.g., qwen2.5-7b-instruct"
          value={state.modelName}
          onChange={(e) => setPersistedState({ modelName: e.target.value })}
        />
        <p className="text-sm text-gray-500 mt-1">
          A friendly name to identify this model
        </p>
      </div>
      <Alert>
        <AlertDescription>
          This name will be used to identify the model in the UI and API calls.
        </AlertDescription>
      </Alert>
    </div>
  );

  // Step 2: Model Weights
  const WeightsStep = () => (
    <div className="space-y-4">
      <div>
        <Label htmlFor="weightsPath">Weights File Path</Label>
        <Input
          id="weightsPath"
          placeholder="/path/to/model/weights.safetensors"
          value={state.weightsPath}
          onChange={(e) => setPersistedState({ weightsPath: e.target.value })}
        />
        <p className="text-sm text-gray-500 mt-1">
          Absolute path to SafeTensors weights file
        </p>
      </div>
      <Alert>
        <FileCheck className="h-4 w-4" />
        <AlertDescription>
          File must be in SafeTensors format (.safetensors)
        </AlertDescription>
      </Alert>
    </div>
  );

  // Step 3: Configuration Files
  const ConfigStep = () => (
    <div className="space-y-4">
      <div>
        <Label htmlFor="configPath">Config File Path</Label>
        <Input
          id="configPath"
          placeholder="/path/to/model/config.json"
          value={state.configPath}
          onChange={(e) => setPersistedState({ configPath: e.target.value })}
        />
      </div>
      <div>
        <Label htmlFor="tokenizerPath">Tokenizer File Path</Label>
        <Input
          id="tokenizerPath"
          placeholder="/path/to/model/tokenizer.json"
          value={state.tokenizerPath}
          onChange={(e) => setPersistedState({ tokenizerPath: e.target.value })}
        />
      </div>
      <div>
        <Label htmlFor="tokenizerConfigPath">Tokenizer Config (Optional)</Label>
        <Input
          id="tokenizerConfigPath"
          placeholder="/path/to/model/tokenizer_config.json"
          value={state.tokenizerConfigPath}
          onChange={(e) => setPersistedState({ tokenizerConfigPath: e.target.value })}
        />
      </div>
    </div>
  );

  // Step 4: Validation & Review
  const ReviewStep = () => (
    <div className="space-y-4">
      <h3 className="font-semibold">Review Import Details</h3>
      <div className="bg-gray-50 p-4 rounded-md space-y-2">
        <div><strong>Model Name:</strong> {state.modelName}</div>
        <div><strong>Weights:</strong> {state.weightsPath}</div>
        <div><strong>Config:</strong> {state.configPath}</div>
        <div><strong>Tokenizer:</strong> {state.tokenizerPath}</div>
      </div>
      <Alert>
        <CheckCircle className="h-4 w-4" />
        <AlertDescription>
          Click "Import" to begin the import process. This may take several minutes.
        </AlertDescription>
      </Alert>
    </div>
  );

  // Step 5: Import Progress
  const ProgressStep = () => (
    <div className="space-y-4">
      <h3 className="font-semibold">Importing Model...</h3>
      <Progress value={75} className="w-full" />
      <p className="text-sm text-gray-600">
        Validating model files and importing into registry...
      </p>
    </div>
  );

  const handleComplete = async () => {
    setIsLoading(true);
    setWizardError(null);
    try {
      const request: ImportModelRequest = {
        model_name: state.modelName,
        weights_path: state.weightsPath,
        config_path: state.configPath,
        tokenizer_path: state.tokenizerPath,
        tokenizer_config_path: state.tokenizerConfigPath || undefined,
        metadata: state.metadata,
      };

      // Start progress tracking and cancellable operation
      const tempOperationId = startModelImport('model_import', `temp_${Date.now()}`, tenantId || 'default');

      await startCancellableImport(async (signal) => {
        const response = await apiClient.importModel(request, {}, false, signal);

        // Success - clear persisted state
        clearPersistedState();
        onComplete(response.import_id);
      }, `model-import-${state.modelName}`);

    } catch (err) {
      if (err) { // Only set error if not cancelled
        const error = err instanceof Error ? err : new Error('Import failed');
        setWizardError(error);
      }
    } finally {
      setIsLoading(false);
    }
  };

  const steps: WizardStep[] = [
    {
      id: 'name',
      title: 'Model Name',
      description: 'Choose a name for this model',
      component: <ModelNameStep />,
      validate: () => {
        if (!state.modelName.trim()) {
          setValidationError('Model name is required');
          return false;
        }
        setValidationError(null);
        return true;
      },
    },
    {
      id: 'weights',
      title: 'Model Weights',
      description: 'Specify the weights file location',
      component: <WeightsStep />,
      validate: () => {
        if (!state.weightsPath.trim()) {
          setValidationError('Weights path is required');
          return false;
        }
        if (!state.weightsPath.endsWith('.safetensors')) {
          setValidationError('Weights file must be .safetensors format');
          return false;
        }
        setValidationError(null);
        return true;
      },
    },
    {
      id: 'config',
      title: 'Configuration',
      description: 'Specify configuration files',
      component: <ConfigStep />,
      validate: () => {
        if (!state.configPath.trim() || !state.tokenizerPath.trim()) {
          setValidationError('Config and tokenizer paths are required');
          return false;
        }
        setValidationError(null);
        return true;
      },
    },
    {
      id: 'review',
      title: 'Review',
      description: 'Confirm import details',
      component: <ReviewStep />,
    },
  ];

  return (
    <div className="space-y-4">
      {/* Resume Dialog */}
      <Dialog open={showResumeDialog} onOpenChange={setShowResumeDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <RotateCcw className="h-5 w-5" />
              Resume Previous Session?
            </DialogTitle>
            <DialogDescription>
              We found a saved model import configuration from a previous session. Would you like to resume where you left off?
            </DialogDescription>
          </DialogHeader>
          {savedState && (
            <div className="space-y-2 text-sm">
              <div className="flex items-center gap-2">
                <span className="text-muted-foreground">Model:</span>
                <span className="font-medium">{savedState.modelName || 'Untitled'}</span>
              </div>
              {savedState.currentStep !== undefined && (
                <div className="flex items-center gap-2">
                  <span className="text-muted-foreground">Progress:</span>
                  <span className="font-medium">Step {savedState.currentStep + 1} of 4</span>
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

      {wizardError && errorRecoveryTemplates.genericError(
        wizardError,
        () => setWizardError(null)
      )}

      {validationError && (
        <ErrorRecovery
          error={validationError}
          onRetry={() => setValidationError(null)}
        />
      )}

      <Wizard
        steps={steps}
        currentStep={currentStep}
        onStepChange={setCurrentStep}
        onComplete={handleComplete}
        onCancel={onCancel}
        title="Import Base Model"
        completeButtonText="Import Model"
        isLoading={isLoading}
      />
    </div>
  );
}

export default ModelImportWizard;

