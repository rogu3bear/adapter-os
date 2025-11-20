//! Adapter Import Wizard Component
//!
//! Multi-step wizard for importing adapter files with validation and configuration.
//!
//! Citations:
//! - ui/src/components/ModelImportWizard.tsx - Wizard pattern reference
//! - ui/src/components/Adapters.tsx L1371-L1450 - Current simple import implementation

import React, { useState } from 'react';
import { Wizard, WizardStep } from '@/components/ui/wizard';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Progress } from '@/components/ui/progress';
import { Switch } from '@/components/ui/switch';
import { ErrorRecovery, ErrorRecoveryTemplates } from './ui/error-recovery';
import { Upload, FileCheck, Settings, CheckCircle } from 'lucide-react';
import apiClient from '@/api/client';
import { Adapter } from '@/api/types';
import { useProgressOperation } from '../hooks/useProgressOperation';
import { useCancellableOperation } from '../hooks/useCancellableOperation';

interface AdapterImportWizardProps {
  onComplete: (adapter: Adapter) => void;
  onCancel: () => void;
  tenantId?: string;
}

interface WizardState {
  file: File | null;
  autoLoad: boolean;
  filePreview: {
    name: string;
    size: number;
    type: string;
  } | null;
}

export function AdapterImportWizard({ onComplete, onCancel, tenantId }: AdapterImportWizardProps) {
  const [currentStep, setCurrentStep] = useState(0);
  const [isLoading, setIsLoading] = useState(false);
  const [wizardError, setWizardError] = useState<Error | null>(null);
  const [validationError, setValidationError] = useState<string | null>(null);
  const [state, setState] = useState<WizardState>({
    file: null,
    autoLoad: true,
    filePreview: null,
  });

  // Progress tracking for file upload
  const { start: startFileUpload } = useProgressOperation();

  // Cancellation support for file upload
  const { start: startCancellableUpload, cancel: cancelUpload } = useCancellableOperation();

  const fileInputRef = React.useRef<HTMLInputElement>(null);

  // Step 1: File Upload
  const FileUploadStep = () => (
    <div className="space-y-4">
      <div>
        <Label htmlFor="adapterFile">Adapter File</Label>
        <div className="mt-2">
          <input
            ref={fileInputRef}
            type="file"
            id="adapterFile"
            accept=".aos,.safetensors"
            className="hidden"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) {
                setState({
                  ...state,
                  file,
                  filePreview: {
                    name: file.name,
                    size: file.size,
                    type: file.type || 'application/octet-stream',
                  },
                });
                setValidationError(null);
              }
            }}
          />
          <div
            className="border-2 border-dashed border-gray-300 rounded-lg p-8 text-center cursor-pointer hover:border-gray-400 transition-colors"
            onClick={() => fileInputRef.current?.click()}
          >
            {state.filePreview ? (
              <div className="space-y-2">
                <FileCheck className="h-12 w-12 text-gray-600 mx-auto" />
                <p className="font-medium">{state.filePreview.name}</p>
                <p className="text-sm text-muted-foreground">
                  {(state.filePreview.size / 1024 / 1024).toFixed(2)} MB
                </p>
              </div>
            ) : (
              <div className="space-y-2">
                <Upload className="h-12 w-12 text-gray-400 mx-auto" />
                <p className="text-sm text-muted-foreground">
                  Click to select or drag and drop adapter file (.aos or .safetensors)
                </p>
              </div>
            )}
          </div>
        </div>
      </div>
      <Alert>
        <FileCheck className="h-4 w-4" />
        <AlertDescription>
          Supported formats: .aos (AdapterOS format) or .safetensors (SafeTensors format)
        </AlertDescription>
      </Alert>
    </div>
  );

  // Step 2: Configuration
  const ConfigurationStep = () => (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="space-y-0.5">
          <Label htmlFor="autoLoad">Auto-load After Import</Label>
          <p className="text-sm text-muted-foreground">
            Automatically load the adapter into memory after import
          </p>
        </div>
        <Switch
          id="autoLoad"
          checked={state.autoLoad}
          onCheckedChange={(checked) => setState({ ...state, autoLoad: checked })}
        />
      </div>
      <Alert>
        <Settings className="h-4 w-4" />
        <AlertDescription>
          If enabled, the adapter will be immediately available for inference after import.
        </AlertDescription>
      </Alert>
    </div>
  );

  // Step 3: Validation Preview
  const ValidationStep = () => (
    <div className="space-y-4">
      <h3 className="font-semibold">Import Preview</h3>
      <div className="bg-gray-50 p-4 rounded-md space-y-2">
        <div><strong>File Name:</strong> {state.filePreview?.name}</div>
        <div><strong>File Size:</strong> {state.filePreview ? (state.filePreview.size / 1024 / 1024).toFixed(2) + ' MB' : 'N/A'}</div>
        <div><strong>Auto-load:</strong> {state.autoLoad ? 'Yes' : 'No'}</div>
      </div>
      <Alert>
        <CheckCircle className="h-4 w-4" />
        <AlertDescription>
          Click "Import" to begin the import process. This may take a few moments.
        </AlertDescription>
      </Alert>
    </div>
  );

  // Step 4: Import Progress
  const ProgressStep = () => (
    <div className="space-y-4">
      <h3 className="font-semibold">Importing Adapter...</h3>
      <Progress value={isLoading ? undefined : 100} className="w-full" />
      <p className="text-sm text-gray-600">
        {isLoading ? 'Uploading and validating adapter file...' : 'Import complete!'}
      </p>
    </div>
  );

  const handleComplete = async () => {
    if (!state.file) {
      setValidationError('Please select a file to import');
      return;
    }

    setIsLoading(true);
    setWizardError(null);
    try {
      // Start progress tracking for file upload
      const tempOperationId = startFileUpload('file_upload', `upload_${Date.now()}`, tenantId || 'default');

      await startCancellableUpload(async (signal) => {
        const adapter = await apiClient.importAdapter(state.file, state.autoLoad, {}, false, signal);
        onComplete(adapter);
      }, `file-upload-${state.file.name}`);

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
      id: 'upload',
      title: 'File Upload',
      description: 'Select adapter file to import',
      component: <FileUploadStep />,
      validate: () => {
        if (!state.file) {
          setValidationError('Please select a file to import');
          return false;
        }
        const validExtensions = ['.aos', '.safetensors'];
        const fileName = state.file.name.toLowerCase();
        const hasValidExtension = validExtensions.some(ext => fileName.endsWith(ext));
        if (!hasValidExtension) {
          setValidationError('File must be .aos or .safetensors format');
          return false;
        }
        setValidationError(null);
        return true;
      },
    },
    {
      id: 'configure',
      title: 'Configuration',
      description: 'Set import options',
      component: <ConfigurationStep />,
    },
    {
      id: 'review',
      title: 'Review',
      description: 'Confirm import details',
      component: <ValidationStep />,
    },
  ];

  // Show progress step if loading
  const displaySteps = isLoading 
    ? [...steps, {
        id: 'progress',
        title: 'Importing',
        description: 'Processing import',
        component: <ProgressStep />,
      }]
    : steps;

  return (
    <div className="space-y-4">
      {wizardError && (
        <ErrorRecovery
          error={wizardError.message}
          onRetry={() => setWizardError(null)}
        />
      )}

      {validationError && (
        <ErrorRecovery
          error={validationError}
          onRetry={() => setValidationError(null)}
        />
      )}

      <Wizard
        steps={displaySteps}
        currentStep={isLoading ? displaySteps.length - 1 : currentStep}
        onStepChange={setCurrentStep}
        onComplete={handleComplete}
        onCancel={onCancel}
        title="Import Adapter"
        completeButtonText={isLoading ? "Importing..." : "Import Adapter"}
        isLoading={isLoading}
      />
    </div>
  );
}

export default AdapterImportWizard;
