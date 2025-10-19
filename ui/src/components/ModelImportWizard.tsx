import React, { useState } from 'react';
import { Wizard, WizardStep } from './ui/wizard';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Alert, AlertDescription } from './ui/alert';
import { toast } from 'sonner';
import { Upload, FileCheck, CheckCircle, AlertTriangle } from 'lucide-react';
import apiClient from '../api/client';
import { ImportModelRequest } from '../api/types';

interface ModelImportWizardProps {
  onComplete: (importId: string) => void;
  onCancel: () => void;
}

interface WizardState {
  modelName: string;
  weightsPath: string;
  configPath: string;
  tokenizerPath: string;
  tokenizerConfigPath: string;
  metadata: Record<string, any>;
}

export function ModelImportWizard({ onComplete, onCancel }: ModelImportWizardProps) {
  const [currentStep, setCurrentStep] = useState(0);
  const [isLoading, setIsLoading] = useState(false);
  const [state, setState] = useState<WizardState>({
    modelName: '',
    weightsPath: '',
    configPath: '',
    tokenizerPath: '',
    tokenizerConfigPath: '',
    metadata: {},
  });

  // Step 1: Model Name
  const ModelNameStep = () => (
    <div className="space-y-4">
      <div>
        <Label htmlFor="modelName">Model Name</Label>
        <Input
          id="modelName"
          placeholder="e.g., qwen2.5-7b-instruct"
          value={state.modelName}
          onChange={(e) => setState({ ...state, modelName: e.target.value })}
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
          onChange={(e) => setState({ ...state, weightsPath: e.target.value })}
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
          onChange={(e) => setState({ ...state, configPath: e.target.value })}
        />
      </div>
      <div>
        <Label htmlFor="tokenizerPath">Tokenizer File Path</Label>
        <Input
          id="tokenizerPath"
          placeholder="/path/to/model/tokenizer.json"
          value={state.tokenizerPath}
          onChange={(e) => setState({ ...state, tokenizerPath: e.target.value })}
        />
      </div>
      <div>
        <Label htmlFor="tokenizerConfigPath">Tokenizer Config (Optional)</Label>
        <Input
          id="tokenizerConfigPath"
          placeholder="/path/to/model/tokenizer_config.json"
          value={state.tokenizerConfigPath}
          onChange={(e) => setState({ ...state, tokenizerConfigPath: e.target.value })}
        />
      </div>
    </div>
  );

  // Step 4: Review
  const ReviewStep = () => (
    <div className="space-y-4">
      <h3 className="font-semibold">Review Import Details</h3>
      <div className="bg-gray-50 p-4 rounded-md space-y-2">
        <div><strong>Model Name:</strong> {state.modelName}</div>
        <div><strong>Weights:</strong> {state.weightsPath}</div>
        <div><strong>Config:</strong> {state.configPath}</div>
        <div><strong>Tokenizer:</strong> {state.tokenizerPath}</div>
        {state.tokenizerConfigPath && (
          <div><strong>Tokenizer Config:</strong> {state.tokenizerConfigPath}</div>
        )}
      </div>
      <Alert>
        <CheckCircle className="h-4 w-4" />
        <AlertDescription>
          Click "Import" to begin the import process. This may take several minutes.
        </AlertDescription>
      </Alert>
    </div>
  );

  const handleComplete = async () => {
    setIsLoading(true);
    try {
      const request: ImportModelRequest = {
        model_name: state.modelName,
        weights_path: state.weightsPath,
        config_path: state.configPath,
        tokenizer_path: state.tokenizerPath,
        tokenizer_config_path: state.tokenizerConfigPath || undefined,
        metadata: state.metadata,
      };

      const response = await apiClient.importModel(request);
      toast.success(`Model import started: ${response.import_id}`);
      onComplete(response.import_id);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Import failed';
      toast.error(errorMsg);
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
          toast.error('Model name is required');
          return false;
        }
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
          toast.error('Weights path is required');
          return false;
        }
        if (!state.weightsPath.endsWith('.safetensors')) {
          toast.error('Weights file must be .safetensors format');
          return false;
        }
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
          toast.error('Config and tokenizer paths are required');
          return false;
        }
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
  );
}

