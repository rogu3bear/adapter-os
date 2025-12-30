import React, { useState, useEffect } from 'react';
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

  // Track step transitions for fade animation
  const [isTransitioning, setIsTransitioning] = useState(false);
  const [displayedStep, setDisplayedStep] = useState(currentStep);

  useEffect(() => {
    if (currentStep !== displayedStep) {
      // Start fade out
      setIsTransitioning(true);
      // After fade out completes, update displayed step and fade in
      const timer = setTimeout(() => {
        setDisplayedStep(currentStep);
        setIsTransitioning(false);
      }, 150); // Match the CSS transition duration
      return () => clearTimeout(timer);
    }
    return undefined;
  }, [currentStep, displayedStep]);

  return (
    <div className="space-y-6">
      {/* Wizard Header */}
      {title && (
        <div className="mb-6">
          <h2 className="text-2xl font-bold">{title}</h2>
        </div>
      )}

      {/* Step Indicator */}
      <nav aria-label="Wizard progress" className="mb-8">
        <div
          role="tablist"
          aria-label={`${steps.length} step wizard`}
          className="flex items-center justify-between overflow-x-auto pb-2 scrollbar-thin scrollbar-thumb-muted"
        >
          {steps.map((step, index) => {
            const isCompleted = index < currentStep;
            const isCurrent = index === currentStep;
            const stepStatus = isCompleted ? 'completed' : isCurrent ? 'current' : 'upcoming';

            return (
              <React.Fragment key={step.id}>
                <div
                  role="tab"
                  aria-selected={isCurrent}
                  aria-current={isCurrent ? 'step' : undefined}
                  aria-label={`Step ${index + 1} of ${steps.length}: ${step.title}${step.description ? `, ${step.description}` : ''} (${stepStatus})`}
                  tabIndex={isCurrent ? 0 : -1}
                  className="flex flex-col items-center flex-1 min-w-[80px] md:min-w-[120px]"
                >
                  <div
                    className={`flex items-center justify-center w-10 h-10 rounded-full border-2 transition-colors ${
                      isCompleted
                        ? 'bg-primary border-primary text-primary-foreground'
                        : isCurrent
                        ? 'bg-primary border-primary text-primary-foreground'
                        : 'bg-background border-muted-foreground text-muted-foreground'
                    }`}
                    aria-hidden="true"
                  >
                    {isCompleted ? (
                      <CheckCircle className="h-5 w-5" />
                    ) : (
                      <Circle className="h-5 w-5" />
                    )}
                  </div>
                  <div className="mt-2 text-center">
                    <p
                      className={`text-xs md:text-sm font-medium whitespace-nowrap ${
                        index <= currentStep ? 'text-foreground' : 'text-muted-foreground'
                      }`}
                    >
                      {step.title}
                    </p>
                    {step.description && (
                      <p className="hidden md:block text-xs text-muted-foreground">{step.description}</p>
                    )}
                  </div>
                </div>
                {index < steps.length - 1 && (
                  <div
                    aria-hidden="true"
                    className={`flex-1 h-0.5 mx-2 md:mx-4 min-w-[20px] transition-colors ${
                      isCompleted ? 'bg-primary' : 'bg-muted'
                    }`}
                  />
                )}
              </React.Fragment>
            );
          })}
        </div>
      </nav>

      {/* Step Content */}
      <Card>
        <CardHeader>
          <CardTitle>{steps[currentStep].title}</CardTitle>
          {steps[currentStep].description && (
            <p className="text-sm text-muted-foreground">{steps[currentStep].description}</p>
          )}
        </CardHeader>
        <CardContent className="min-h-[400px]">
          <div
            className={`transition-opacity duration-150 ease-in-out ${
              isTransitioning ? 'opacity-0' : 'opacity-100'
            }`}
          >
            {steps[displayedStep].component}
          </div>
        </CardContent>
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
