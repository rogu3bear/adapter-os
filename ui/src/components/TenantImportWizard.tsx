import React, { useState } from 'react';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Textarea } from './ui/textarea';
import { Alert, AlertDescription } from './ui/alert';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { Upload, FileCheck, CheckCircle, XCircle, ArrowRight, ArrowLeft, FileText } from 'lucide-react';
import apiClient from '@/api/client';
import { toast } from 'sonner';

interface TenantImportWizardProps {
  onComplete: (tenant: { id: string; name: string }) => void;
  onCancel: () => void;
}

interface WizardState {
  importMethod: 'file' | 'manual';
  file: File | null;
  tenantData: {
    name: string;
    description?: string;
    dataClassification: 'public' | 'internal' | 'confidential' | 'restricted';
    itarCompliant: boolean;
  };
}

export function TenantImportWizard({ onComplete, onCancel }: TenantImportWizardProps) {
  const [currentStep, setCurrentStep] = useState(0);
  const [isLoading, setIsLoading] = useState(false);
  const [wizardError, setWizardError] = useState<Error | null>(null);
  const [validationError, setValidationError] = useState<string | null>(null);
  const [state, setState] = useState<WizardState>({
    importMethod: 'manual',
    file: null,
    tenantData: {
      name: '',
      description: '',
      dataClassification: 'internal',
      itarCompliant: false,
    },
  });

  // Step 1: Import Method
  const ImportMethodStep = () => (
    <div className="space-y-4">
      <div>
        <Label>Import Method</Label>
        <div className="grid grid-cols-2 gap-4 mt-2">
          <button
            type="button"
            onClick={() => setState({ ...state, importMethod: 'manual' })}
            className={`p-4 border-2 rounded-lg text-left transition-colors ${
              state.importMethod === 'manual'
                ? 'border-primary bg-primary/5'
                : 'border-gray-200 hover:border-gray-300'
            }`}
          >
            <FileText className="h-8 w-8 mb-2 text-gray-400" />
            <div className="font-medium">Manual Entry</div>
            <div className="text-sm text-muted-foreground">
              Enter organization details manually
            </div>
          </button>
          <button
            type="button"
            onClick={() => setState({ ...state, importMethod: 'file' })}
            className={`p-4 border-2 rounded-lg text-left transition-colors ${
              state.importMethod === 'file'
                ? 'border-primary bg-primary/5'
                : 'border-gray-200 hover:border-gray-300'
            }`}
          >
            <Upload className="h-8 w-8 mb-2 text-gray-400" />
            <div className="font-medium">Import from File</div>
            <div className="text-sm text-muted-foreground">
              Upload JSON configuration
            </div>
          </button>
        </div>
      </div>
    </div>
  );

  // Step 2a: File Upload
  const FileUploadStep = () => (
    <div className="space-y-4">
      <div>
        <Label htmlFor="tenantFile">Organization Configuration File (JSON)</Label>
        <div className="border-2 border-dashed border-gray-300 rounded-lg p-8 text-center">
          <input
            type="file"
            id="tenantFile"
            accept=".json"
            className="hidden"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) {
                const reader = new FileReader();
                reader.onload = (event) => {
                  try {
                    const json = JSON.parse(event.target?.result as string);
                    if (json.name) {
                      setState({
                        ...state,
                        file,
                        tenantData: {
                          name: json.name || '',
                          description: json.description || '',
                          dataClassification: json.dataClassification || json.data_classification || 'internal',
                          itarCompliant: json.itarCompliant || json.itar_compliant || false,
                        },
                      });
                    } else {
                      setValidationError('Invalid organization configuration file');
                    }
                  } catch (err) {
                    setValidationError('Failed to parse JSON file');
                  }
                };
                reader.onerror = () => {
                  setValidationError('Failed to read configuration file');
                };
                reader.readAsText(file);
              }
            }}
          />
          <label htmlFor="tenantFile" className="cursor-pointer">
            <Upload className="h-12 w-12 mx-auto mb-4 text-gray-400" />
            <div className="text-sm font-medium text-gray-700 mb-2">
              {state.file ? state.file.name : 'Click to select JSON file'}
            </div>
            <div className="text-xs text-gray-500">
              JSON format with organization configuration
            </div>
          </label>
        </div>
        {state.file && (
          <div className="mt-2 flex items-center gap-2 text-sm text-green-600">
            <FileCheck className="h-4 w-4" />
            <span>{state.file.name}</span>
          </div>
        )}
      </div>
      <Alert>
        <AlertDescription>
          The JSON file should contain organization configuration with at least a "name" field.
        </AlertDescription>
      </Alert>
    </div>
  );

  // Step 2b: Manual Entry
  const ManualEntryStep = () => (
    <div className="space-y-4">
      <div>
        <Label htmlFor="tenantName">Organization Name *</Label>
        <Input
          id="tenantName"
          placeholder="e.g., Engineering Team"
          value={state.tenantData.name}
          onChange={(e) =>
            setState({
              ...state,
              tenantData: { ...state.tenantData, name: e.target.value },
            })
          }
        />
      </div>
      <div>
        <Label htmlFor="tenantDescription">Description</Label>
        <Textarea
          id="tenantDescription"
          placeholder="Describe the organization's purpose..."
          value={state.tenantData.description}
          onChange={(e) =>
            setState({
              ...state,
              tenantData: { ...state.tenantData, description: e.target.value },
            })
          }
        />
      </div>
      <div>
        <Label htmlFor="dataClassification">Data Classification</Label>
        <Select
          value={state.tenantData.dataClassification}
          onValueChange={(value: 'public' | 'internal' | 'confidential' | 'restricted') =>
            setState({
              ...state,
              tenantData: { ...state.tenantData, dataClassification: value },
            })
          }
        >
          <SelectTrigger>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="public">Public</SelectItem>
            <SelectItem value="internal">Internal</SelectItem>
            <SelectItem value="confidential">Confidential</SelectItem>
            <SelectItem value="restricted">Restricted</SelectItem>
          </SelectContent>
        </Select>
      </div>
      <div className="flex items-center justify-between p-4 border rounded-lg">
        <div>
          <Label htmlFor="itarCompliant">ITAR Compliant</Label>
          <p className="text-sm text-muted-foreground">
            This organization handles ITAR-restricted data
          </p>
        </div>
        <input
          type="checkbox"
          id="itarCompliant"
          checked={state.tenantData.itarCompliant}
          onChange={(e) =>
            setState({
              ...state,
              tenantData: { ...state.tenantData, itarCompliant: e.target.checked },
            })
          }
          className="h-4 w-4"
        />
      </div>
    </div>
  );

  // Step 3: Review
  const ReviewStep = () => (
    <div className="space-y-4">
      <h3 className="font-semibold">Review Organization Configuration</h3>
      <div className="bg-gray-50 p-4 rounded-md space-y-2">
        <div><strong>Name:</strong> {state.tenantData.name}</div>
        {state.tenantData.description && (
          <div><strong>Description:</strong> {state.tenantData.description}</div>
        )}
        <div><strong>Data Classification:</strong> {state.tenantData.dataClassification}</div>
        <div><strong>ITAR Compliant:</strong> {state.tenantData.itarCompliant ? 'Yes' : 'No'}</div>
      </div>
      <Alert>
        <CheckCircle className="h-4 w-4" />
        <AlertDescription>
          Click "Create Organization" to complete the import.
        </AlertDescription>
      </Alert>
    </div>
  );

  const handleNext = () => {
    if (currentStep === 0) {
      // Method selection - always valid
      setValidationError(null);
    } else if (currentStep === 1) {
      // File upload or manual entry validation
      if (state.importMethod === 'file' && !state.file) {
        setValidationError('Please select an organization configuration file');
        return;
      } else if (state.importMethod === 'manual' && !state.tenantData.name.trim()) {
        setValidationError('Organization name is required');
        return;
      }
      setValidationError(null);
    }
    setCurrentStep(prev => prev + 1);
  };

  const handleBack = () => {
    setCurrentStep(prev => prev - 1);
  };

  const handleComplete = async () => {
    if (!state.tenantData.name.trim()) {
      setValidationError('Organization name is required');
      return;
    }

    setIsLoading(true);
    setWizardError(null);

    try {
      const tenant = await apiClient.createTenant({
        name: state.tenantData.name,
        isolation_level: 'standard',
      });

      toast.success(`Organization "${tenant.name}" created successfully`);
      onComplete(tenant);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Import failed');
      setWizardError(error);
      toast.error(`Failed to create organization: ${error.message}`);
    } finally {
      setIsLoading(false);
    }
  };

  const steps = [
    { id: 'method', title: 'Import Method', component: <ImportMethodStep /> },
    {
      id: 'config',
      title: state.importMethod === 'file' ? 'Upload File' : 'Organization Details',
      component: state.importMethod === 'file' ? <FileUploadStep /> : <ManualEntryStep />,
    },
    { id: 'review', title: 'Review', component: <ReviewStep /> },
  ];

  const currentStepData = steps[currentStep];

  return (
    <div className="space-y-6">
      {/* Progress indicator */}
      <div className="flex items-center justify-between text-sm">
        <div className="flex items-center gap-2">
          {steps.map((step, idx) => (
            <React.Fragment key={step.id}>
              <div
                className={`flex items-center gap-2 ${
                  idx === currentStep
                    ? 'font-semibold text-primary'
                    : idx < currentStep
                    ? 'text-green-600'
                    : 'text-muted-foreground'
                }`}
              >
                {idx < currentStep ? (
                  <CheckCircle className="h-4 w-4" />
                ) : (
                  <div className={`h-4 w-4 rounded-full border-2 ${
                    idx === currentStep ? 'border-primary' : 'border-muted-foreground'
                  }`} />
                )}
                <span>{step.title}</span>
              </div>
              {idx < steps.length - 1 && (
                <ArrowRight className="h-4 w-4 text-muted-foreground" />
              )}
            </React.Fragment>
          ))}
        </div>
        <div className="text-muted-foreground">
          Step {currentStep + 1} of {steps.length}
        </div>
      </div>

      {wizardError && (
        <ErrorRecovery
          error={wizardError.message}
          onRetry={() => setWizardError(null)}
        />
      )}

      {validationError && (
        <Alert variant="destructive">
          <XCircle className="h-4 w-4" />
          <AlertDescription>{validationError}</AlertDescription>
        </Alert>
      )}

      {/* Current step content */}
      <div className="min-h-[300px]">{currentStepData?.component}</div>

      {/* Navigation buttons */}
      <div className="flex justify-between">
        <Button
          variant="outline"
          onClick={currentStep === 0 ? onCancel : handleBack}
          disabled={isLoading}
        >
          <ArrowLeft className="h-4 w-4 mr-2" />
          {currentStep === 0 ? 'Cancel' : 'Back'}
        </Button>
        {currentStep < steps.length - 1 ? (
          <Button onClick={handleNext} disabled={isLoading}>
            Next
            <ArrowRight className="h-4 w-4 ml-2" />
          </Button>
        ) : (
          <Button onClick={handleComplete} disabled={isLoading || !state.tenantData.name.trim()}>
            {isLoading ? 'Creating...' : 'Create Organization'}
          </Button>
        )}
      </div>
    </div>
  );
}

