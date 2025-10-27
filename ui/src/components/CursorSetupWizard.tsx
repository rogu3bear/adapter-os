import React, { useState, useEffect } from 'react';
import { Wizard, WizardStep } from '@/components/ui/wizard';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Badge } from './ui/badge';
import { toast } from 'sonner';
import { CheckCircle, XCircle, Copy, ExternalLink } from 'lucide-react';
import apiClient from '@/api/client';
import { CursorConfigResponse } from '@/api/types';

interface CursorSetupWizardProps {
  onComplete: () => void;
  onCancel: () => void;
}

export function CursorSetupWizard({ onComplete, onCancel }: CursorSetupWizardProps) {
  const [currentStep, setCurrentStep] = useState(0);
  const [isLoading, setIsLoading] = useState(false);
  const [config, setConfig] = useState<CursorConfigResponse | null>(null);

  useEffect(() => {
    loadConfig();
  }, []);

  const loadConfig = async () => {
    try {
      const configData = await apiClient.getCursorConfig();
      setConfig(configData);
    } catch (err) {
      toast.error('Failed to load Cursor configuration');
    }
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
    toast.success('Copied to clipboard');
  };

  // Step 1: Prerequisites Check
  const PrerequisitesStep = () => (
    <div className="space-y-4">
      <h3 className="font-semibold">Prerequisites</h3>
      <div className="space-y-2">
        <div className="flex items-center gap-2">
          {config?.is_ready ? (
            <CheckCircle className="h-5 w-5 text-green-500" />
          ) : (
            <XCircle className="h-5 w-5 text-red-500" />
          )}
          <span>Base model loaded</span>
        </div>
        <div className="flex items-center gap-2">
          <CheckCircle className="h-5 w-5 text-green-500" />
          <span>API server running</span>
        </div>
      </div>
      {!config?.is_ready && (
        <Alert variant="destructive">
          <AlertDescription>
            Please load a base model before configuring Cursor
          </AlertDescription>
        </Alert>
      )}
    </div>
  );

  // Step 2: API Endpoint Configuration
  const EndpointStep = () => (
    <div className="space-y-4">
      <h3 className="font-semibold">API Endpoint</h3>
      <div className="bg-gray-50 p-4 rounded-md space-y-2">
        <Label>Endpoint URL</Label>
        <div className="flex gap-2">
          <code className="flex-1 bg-white p-2 rounded border">
            {config?.api_endpoint}
          </code>
          <Button
            size="sm"
            variant="outline"
            onClick={() => copyToClipboard(config?.api_endpoint || '')}
          >
            <Copy className="h-4 w-4" />
          </Button>
        </div>
      </div>
      <Alert>
        <AlertDescription>
          This endpoint provides OpenAI-compatible API for Cursor IDE
        </AlertDescription>
      </Alert>
    </div>
  );

  // Step 3: Model Configuration
  const ModelStep = () => (
    <div className="space-y-4">
      <h3 className="font-semibold">Model Name</h3>
      <div className="bg-gray-50 p-4 rounded-md space-y-2">
        <Label>Model Identifier</Label>
        <div className="flex gap-2">
          <code className="flex-1 bg-white p-2 rounded border">
            {config?.model_name}
          </code>
          <Button
            size="sm"
            variant="outline"
            onClick={() => copyToClipboard(config?.model_name || '')}
          >
            <Copy className="h-4 w-4" />
          </Button>
        </div>
      </div>
      <Alert>
        <AlertDescription>
          Use this model name when configuring Cursor's model settings
        </AlertDescription>
      </Alert>
    </div>
  );

  // Step 4: Instructions
  const InstructionsStep = () => (
    <div className="space-y-4">
      <h3 className="font-semibold">Cursor Configuration Steps</h3>
      <ol className="list-decimal list-inside space-y-2">
        {config?.setup_instructions.map((instruction, idx) => (
          <li key={idx} className="text-sm">{instruction}</li>
        ))}
      </ol>
      <Button
        variant="outline"
        className="w-full"
        onClick={() => window.open('https://cursor.sh/settings', '_blank')}
      >
        <ExternalLink className="h-4 w-4 mr-2" />
        Open Cursor Settings
      </Button>
    </div>
  );

  const handleComplete = async () => {
    toast.success('Cursor setup complete!');
    onComplete();
  };

  const steps: WizardStep[] = [
    {
      id: 'prerequisites',
      title: 'Prerequisites',
      description: 'Check system readiness',
      component: <PrerequisitesStep />,
      validate: () => {
        if (!config?.is_ready) {
          toast.error('Please load a base model first');
          return false;
        }
        return true;
      },
    },
    {
      id: 'endpoint',
      title: 'API Endpoint',
      description: 'Configure connection',
      component: <EndpointStep />,
    },
    {
      id: 'model',
      title: 'Model Name',
      description: 'Set model identifier',
      component: <ModelStep />,
    },
    {
      id: 'instructions',
      title: 'Setup Instructions',
      description: 'Configure Cursor IDE',
      component: <InstructionsStep />,
    },
  ];

  return (
    <Wizard
      steps={steps}
      currentStep={currentStep}
      onStepChange={setCurrentStep}
      onComplete={handleComplete}
      onCancel={onCancel}
      title="Cursor IDE Setup"
      completeButtonText="Complete Setup"
      isLoading={isLoading}
    />
  );
}

