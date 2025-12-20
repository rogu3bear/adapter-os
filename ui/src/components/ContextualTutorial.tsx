import React, { useState, useEffect, useRef } from 'react';
import { Dialog, DialogContent } from './ui/dialog';
import { Button } from './ui/button';
import { Progress } from './ui/progress';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { X, ChevronRight, ChevronLeft, Play, Pause, SkipForward } from 'lucide-react';
import { cn } from '@/lib/utils';

export interface TutorialStep {
  id: string;
  title: string;
  content: string;
  targetSelector?: string; // CSS selector for element to highlight
  targetElement?: HTMLElement | null; // Direct element reference
  position?: 'top' | 'bottom' | 'left' | 'right' | 'center';
  action?: () => void; // Optional action to trigger
  waitForAction?: boolean; // Wait for user to complete action before next step
}

export interface TutorialConfig {
  id: string;
  title: string;
  description: string;
  steps: TutorialStep[];
  trigger?: 'manual' | 'auto' | 'on-error';
  dismissible?: boolean;
}

interface ContextualTutorialProps {
  config: TutorialConfig;
  open: boolean;
  onClose: () => void;
  onComplete?: () => void;
}

export function ContextualTutorial({ config, open, onClose, onComplete }: ContextualTutorialProps) {
  const [currentStep, setCurrentStep] = useState(0);
  const [isPaused, setIsPaused] = useState(false);
  const overlayRef = useRef<HTMLDivElement>(null);
  const targetElementRef = useRef<HTMLElement | null>(null);
  const [targetPosition, setTargetPosition] = useState<{ top: number; left: number; width: number; height: number } | null>(null);

  const currentStepData = config.steps[currentStep];
  const progress = ((currentStep + 1) / config.steps.length) * 100;
  const isLastStep = currentStep === config.steps.length - 1;

  // Find and highlight target element
  useEffect(() => {
    if (!open || !currentStepData) return;

    const updateTargetPosition = () => {
      let element: HTMLElement | null = null;

      if (currentStepData.targetElement) {
        element = currentStepData.targetElement;
      } else if (currentStepData.targetSelector) {
        element = document.querySelector<HTMLElement>(currentStepData.targetSelector);
      }

      targetElementRef.current = element;

      if (element) {
        const rect = element.getBoundingClientRect();
        setTargetPosition({
          top: rect.top + window.scrollY,
          left: rect.left + window.scrollX,
          width: rect.width,
          height: rect.height
        });
      } else {
        setTargetPosition(null);
      }
    };

    updateTargetPosition();
    window.addEventListener('scroll', updateTargetPosition);
    window.addEventListener('resize', updateTargetPosition);

    return () => {
      window.removeEventListener('scroll', updateTargetPosition);
      window.removeEventListener('resize', updateTargetPosition);
    };
  }, [open, currentStep, currentStepData]);

  const handleNext = () => {
    if (currentStep < config.steps.length - 1) {
      setCurrentStep(currentStep + 1);
    } else {
      handleComplete();
    }
  };

  const handlePrevious = () => {
    if (currentStep > 0) {
      setCurrentStep(currentStep - 1);
    }
  };

  const handleSkip = () => {
    handleComplete();
  };

  const handleComplete = () => {
    // onComplete is expected to handle API call and storage sync
    // The hook (useContextualTutorial) manages this via completeTutorial()
    onComplete?.();
    onClose();
  };

  if (!open || !currentStepData) return null;

  return (
    <>
      {/* Overlay with spotlight */}
      <div
        ref={overlayRef}
        className="fixed inset-0 z-50 pointer-events-auto"
        style={{
          background: targetPosition
            ? `radial-gradient(circle at ${targetPosition.left + targetPosition.width / 2}px ${targetPosition.top + targetPosition.height / 2}px, transparent 0, transparent ${Math.max(targetPosition.width, targetPosition.height) / 2 + 20}px, rgba(0, 0, 0, 0.5) 100%)`
            : 'rgba(0, 0, 0, 0.5)'
        }}
        onClick={(e) => {
          // Close on overlay click if dismissible
          if (config.dismissible && e.target === overlayRef.current) {
            onClose();
          }
        }}
      >
        {/* Highlight box around target element */}
        {targetPosition && (
          <div
            className="absolute border-2 border-primary shadow-lg rounded-md pointer-events-none"
            style={{
              top: `${targetPosition.top}px`,
              left: `${targetPosition.left}px`,
              width: `${targetPosition.width}px`,
              height: `${targetPosition.height}px`,
              boxShadow: '0 0 0 9999px rgba(0, 0, 0, 0.5), 0 0 20px rgba(59, 130, 246, 0.5)'
            }}
          />
        )}
      </div>

      {/* Tutorial Dialog */}
      <Dialog open={open} onOpenChange={() => {}}>
        <DialogContent
          className="sm:max-w-md"
          style={
            targetPosition && currentStepData.position && currentStepData.position !== 'center'
              ? {
                  position: 'fixed',
                  top: currentStepData.position === 'top'
                    ? `${Math.max(20, targetPosition.top - 200)}px`
                    : currentStepData.position === 'bottom'
                    ? `${targetPosition.top + targetPosition.height + 20}px`
                    : '50%',
                  left: currentStepData.position === 'left'
                    ? `${Math.max(20, targetPosition.left - 350)}px`
                    : currentStepData.position === 'right'
                    ? `${targetPosition.left + targetPosition.width + 20}px`
                    : '50%',
                  transform: 'none',
                  margin: 0
                }
              : {}
          }
          onPointerDownOutside={(e) => {
            if (!config.dismissible) {
              e.preventDefault();
            }
          }}
          onEscapeKeyDown={(e) => {
            if (!config.dismissible) {
              e.preventDefault();
            }
          }}
        >
          <Card>
            <CardHeader className="pb-3">
              <div className="flex items-start justify-between">
                <div className="flex-1">
                  <CardTitle className="text-base">{config.title}</CardTitle>
                  <p className="text-sm text-muted-foreground mt-1">
                    Step {currentStep + 1} of {config.steps.length}
                  </p>
                </div>
                {config.dismissible && (
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={onClose}
                    className="h-6 w-6 p-0"
                  >
                    <X className="h-4 w-4" />
                  </Button>
                )}
              </div>
              <Progress value={progress} className="mt-3" />
            </CardHeader>
            <CardContent className="space-y-4">
              <div>
                <h3 className="font-semibold mb-2">{currentStepData.title}</h3>
                <p className="text-sm text-muted-foreground leading-relaxed">
                  {currentStepData.content}
                </p>
              </div>

              {currentStepData.action && (
                <Button
                  variant="default"
                  onClick={() => {
                    currentStepData.action?.();
                    if (!currentStepData.waitForAction) {
                      setTimeout(handleNext, 500);
                    }
                  }}
                  className="w-full"
                >
                  {currentStepData.waitForAction ? 'Continue after action' : 'Try it'}
                  <ChevronRight className="ml-2 h-4 w-4" />
                </Button>
              )}

              <div className="flex items-center justify-between gap-2 pt-2 border-t">
                <div className="flex gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handlePrevious}
                    disabled={currentStep === 0}
                  >
                    <ChevronLeft className="h-4 w-4 mr-1" />
                    Previous
                  </Button>
                  {!isLastStep && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={handleSkip}
                    >
                      <SkipForward className="h-4 w-4 mr-1" />
                      Skip
                    </Button>
                  )}
                </div>
                <Button
                  variant="default"
                  size="sm"
                  onClick={handleNext}
                >
                  {isLastStep ? 'Complete' : 'Next'}
                  {!isLastStep && <ChevronRight className="ml-1 h-4 w-4" />}
                </Button>
              </div>
            </CardContent>
          </Card>
        </DialogContent>
      </Dialog>
    </>
  );
}

