import React from 'react';
import { Button } from './button';
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from './card';
import { CheckCircle, Circle } from 'lucide-react';

export interface WizardStep {
  id: string;
  title: string;
  description?: string;
  component: React.ReactNode;
  validate?: () => boolean | Promise<boolean>;
}

interface WizardProps {
  steps: WizardStep[];
  currentStep: number;
  onStepChange: (step: number) => void;
  onComplete: () => void | Promise<void>;
  onCancel?: () => void;
  title?: string;
  completeButtonText?: string;
  isLoading?: boolean;
}

export function Wizard({
  steps,
  currentStep,
  onStepChange,
  onComplete,
  onCancel,
  title,
  completeButtonText = 'Complete',
  isLoading = false,
}: WizardProps) {
  const handleNext = async () => {
    const currentStepDef = steps[currentStep];
    if (currentStepDef.validate) {
      const isValid = await currentStepDef.validate();
      if (!isValid) {
        return;
      }
    }

    if (currentStep < steps.length - 1) {
      onStepChange(currentStep + 1);
    } else {
      await onComplete();
    }
  };

  const handleBack = () => {
    if (currentStep > 0) {
      onStepChange(currentStep - 1);
    }
  };

  const isLastStep = currentStep === steps.length - 1;
  const isFirstStep = currentStep === 0;

  return (
    <div className="space-y-6">
      {/* Wizard Header */}
      {title && (
        <div className="mb-6">
          <h2 className="text-2xl font-bold">{title}</h2>
        </div>
      )}

      {/* Step Indicator */}
      <div className="flex items-center justify-between mb-8">
        {steps.map((step, index) => (
          <React.Fragment key={step.id}>
            <div className="flex flex-col items-center flex-1">
              <div
                className={`flex items-center justify-center w-10 h-10 rounded-full border-2 transition-colors ${
                  index < currentStep
                    ? 'bg-primary border-primary text-primary-foreground'
                    : index === currentStep
                    ? 'bg-primary border-primary text-primary-foreground'
                    : 'bg-background border-muted-foreground text-muted-foreground'
                }`}
              >
                {index < currentStep ? (
                  <CheckCircle className="h-5 w-5" />
                ) : (
                  <Circle className="h-5 w-5" />
                )}
              </div>
              <div className="mt-2 text-center">
                <p
                  className={`text-sm font-medium ${
                    index <= currentStep ? 'text-foreground' : 'text-muted-foreground'
                  }`}
                >
                  {step.title}
                </p>
                {step.description && (
                  <p className="text-xs text-muted-foreground">{step.description}</p>
                )}
              </div>
            </div>
            {index < steps.length - 1 && (
              <div
                className={`flex-1 h-0.5 mx-4 transition-colors ${
                  index < currentStep ? 'bg-primary' : 'bg-muted'
                }`}
              />
            )}
          </React.Fragment>
        ))}
      </div>

      {/* Step Content */}
      <Card>
        <CardHeader>
          <CardTitle>{steps[currentStep].title}</CardTitle>
          {steps[currentStep].description && (
            <p className="text-sm text-muted-foreground">{steps[currentStep].description}</p>
          )}
        </CardHeader>
        <CardContent className="min-h-[400px]">{steps[currentStep].component}</CardContent>
        <CardFooter className="flex justify-between">
          <div>
            {onCancel && (
              <Button variant="outline" onClick={onCancel} disabled={isLoading}>
                Cancel
              </Button>
            )}
          </div>
          <div className="flex gap-2">
            <Button
              variant="outline"
              onClick={handleBack}
              disabled={isFirstStep || isLoading}
            >
              Back
            </Button>
            <Button onClick={handleNext} disabled={isLoading}>
              {isLoading ? 'Processing...' : isLastStep ? completeButtonText : 'Next'}
            </Button>
          </div>
        </CardFooter>
      </Card>
    </div>
  );
}
