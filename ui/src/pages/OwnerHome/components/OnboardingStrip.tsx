/**
 * OnboardingStrip - User onboarding and quick start guidance
 *
 * Shows contextual onboarding based on user state:
 * - New users: Full 3-step onboarding guide
 * - Early users (1-3 adapters): Simplified "Create Adapter" CTA
 * - Experienced users: Hidden (use system normally)
 *
 * Dismissible with localStorage persistence.
 */

import React, { useState, useEffect, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Rocket,
  Upload,
  Box,
  Play,
  X,
  ChevronRight,
  CheckCircle,
  PlusCircle,
  Sparkles,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { ROUTE_PATHS, buildAdaptersRegisterLink, buildInferenceLink } from '@/utils/navLinks';

const ONBOARDING_STORAGE_KEY = 'aos-onboarding-dismissed';

interface OnboardingStep {
  id: string;
  title: string;
  description: string;
  icon: React.ElementType;
  action: string;
  path: string;
}

const onboardingSteps: OnboardingStep[] = [
  {
    id: 'import-model',
    title: 'Import a base model',
    description: 'Get started by importing a foundation model like Qwen 2.5 or Llama',
    icon: Upload,
    action: 'Import Model',
    path: ROUTE_PATHS.baseModels,
  },
  {
    id: 'train-adapter',
    title: 'Train your first adapter',
    description: 'Create a custom LoRA adapter to enhance your model with specialized knowledge',
    icon: Box,
    action: 'Create Adapter',
    path: buildAdaptersRegisterLink(),
  },
  {
    id: 'run-inference',
    title: 'Run a demo inference',
    description: 'Test your setup with a sample prompt and see the results',
    icon: Play,
    action: 'Try Inference',
    path: buildInferenceLink(),
  },
];

type UserState = 'new' | 'early' | 'experienced';

interface OnboardingStripProps {
  /** Number of adapters the user has */
  adapterCount?: number;
  /** Whether a model is currently loaded */
  hasModel?: boolean;
  /** Optional className */
  className?: string;
}

export const OnboardingStrip: React.FC<OnboardingStripProps> = ({
  adapterCount = 0,
  hasModel = false,
  className,
}) => {
  const navigate = useNavigate();
  const [dismissed, setDismissed] = useState<boolean>(false);
  const [completedSteps, setCompletedSteps] = useState<Set<string>>(new Set());

  // Determine user state based on adapter count
  const userState = useMemo<UserState>(() => {
    if (adapterCount === 0) return 'new';
    if (adapterCount <= 3) return 'early';
    return 'experienced';
  }, [adapterCount]);

  useEffect(() => {
    const isDismissed = localStorage.getItem(ONBOARDING_STORAGE_KEY);
    if (isDismissed === 'true') {
      setDismissed(true);
    }

    // Check localStorage for completed steps
    const completed = new Set<string>();
    onboardingSteps.forEach((step) => {
      const stepCompleted = localStorage.getItem(`aos-onboarding-${step.id}`);
      if (stepCompleted === 'true') {
        completed.add(step.id);
      }
    });
    setCompletedSteps(completed);
  }, []);

  const handleDismiss = () => {
    localStorage.setItem(ONBOARDING_STORAGE_KEY, 'true');
    setDismissed(true);
  };

  const handleStepToggle = (stepId: string) => {
    const newCompleted = new Set(completedSteps);
    if (newCompleted.has(stepId)) {
      newCompleted.delete(stepId);
      localStorage.removeItem(`aos-onboarding-${stepId}`);
    } else {
      newCompleted.add(stepId);
      localStorage.setItem(`aos-onboarding-${stepId}`, 'true');
    }
    setCompletedSteps(newCompleted);
  };

  const handleStartStep = (path: string) => {
    navigate(path);
  };

  // Don't show for experienced users or if dismissed
  if (dismissed || userState === 'experienced') {
    return null;
  }

  const allStepsCompleted = onboardingSteps.every((step) =>
    completedSteps.has(step.id)
  );

  // Early user view - simplified "Create Adapter" CTA (replaces Hero Card)
  if (userState === 'early') {
    return (
      <Card
        className={cn(
          'border-2 border-info/20 bg-gradient-to-br from-info-surface to-primary-surface',
          className
        )}
      >
        <CardContent className="p-5">
          <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
            <div className="flex items-start gap-3 flex-1">
              <div className="p-2 bg-info-surface rounded-lg flex-shrink-0">
                <Sparkles className="h-5 w-5 text-info" />
              </div>
              <div>
                <h3 className="text-base font-semibold text-slate-900 mb-1">
                  Create Your Custom Adapter
                </h3>
                <p className="text-sm text-slate-600">
                  Train a specialized LoRA adapter in 3 simple steps
                </p>
                <div className="flex flex-wrap gap-3 mt-2 text-xs text-slate-500">
                  <span className="flex items-center gap-1">
                    <CheckCircle className="h-3 w-3 text-success" />
                    Upload data
                  </span>
                  <span className="flex items-center gap-1">
                    <CheckCircle className="h-3 w-3 text-success" />
                    Configure parameters
                  </span>
                  <span className="flex items-center gap-1">
                    <CheckCircle className="h-3 w-3 text-success" />
                    Start training
                  </span>
                </div>
              </div>
            </div>
            <div className="flex items-center gap-2 w-full sm:w-auto">
              <Button
                onClick={() => navigate(buildAdaptersRegisterLink())}
                className="flex-1 sm:flex-none bg-primary hover:bg-primary/90"
              >
                <PlusCircle className="h-4 w-4 mr-1.5" />
                Create Adapter
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={handleDismiss}
                className="h-9 w-9 p-0"
                aria-label="Dismiss"
              >
                <X className="h-4 w-4 text-slate-500" />
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>
    );
  }

  // New user view - full onboarding steps
  return (
    <Card
      className={cn(
        'border-info/20 bg-gradient-to-r from-info-surface to-primary-surface shadow-sm',
        className
      )}
    >
      <CardContent className="p-6">
        <div className="flex items-start justify-between gap-4">
          <div className="flex-1">
            <div className="flex items-center gap-3 mb-4">
              <div className="p-2 bg-info-surface rounded-lg">
                <Rocket className="h-6 w-6 text-info" />
              </div>
              <div>
                <h3 className="text-lg font-semibold text-slate-900">
                  Welcome to AdapterOS!
                </h3>
                <p className="text-sm text-slate-600">
                  Get started with these essential steps
                </p>
              </div>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              {onboardingSteps.map((step, index) => {
                const StepIcon = step.icon;
                const isCompleted = completedSteps.has(step.id);

                return (
                  <div
                    key={step.id}
                    className="flex items-start gap-3 p-4 bg-white rounded-lg border border-slate-200 hover:border-primary/30 transition-colors"
                  >
                    <div className="flex-shrink-0 mt-1">
                      <button
                        onClick={() => handleStepToggle(step.id)}
                        className={cn(
                          'w-5 h-5 rounded border-2 flex items-center justify-center transition-all',
                          isCompleted
                            ? 'bg-primary border-primary'
                            : 'border-slate-300 hover:border-primary/40'
                        )}
                        aria-label={`Mark "${step.title}" as ${isCompleted ? 'incomplete' : 'complete'}`}
                      >
                        {isCompleted && (
                          <svg
                            className="w-3 h-3 text-white"
                            fill="none"
                            strokeLinecap="round"
                            strokeLinejoin="round"
                            strokeWidth="2"
                            viewBox="0 0 24 24"
                            stroke="currentColor"
                          >
                            <path d="M5 13l4 4L19 7" />
                          </svg>
                        )}
                      </button>
                    </div>

                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2 mb-1">
                        <div className="p-1.5 bg-info-surface rounded">
                          <StepIcon className="h-4 w-4 text-info" />
                        </div>
                        <span className="text-xs font-medium text-slate-500">
                          Step {index + 1}
                        </span>
                      </div>
                      <h4
                        className={cn(
                          'text-sm font-semibold mb-1',
                          isCompleted ? 'text-slate-500 line-through' : 'text-slate-900'
                        )}
                      >
                        {step.title}
                      </h4>
                      <p className="text-xs text-slate-600 mb-3 line-clamp-2">
                        {step.description}
                      </p>
                      <Button
                        size="sm"
                        variant={isCompleted ? 'outline' : 'default'}
                        onClick={() => handleStartStep(step.path)}
                        className="w-full group"
                      >
                        {step.action}
                        <ChevronRight className="ml-1 h-3 w-3 group-hover:translate-x-0.5 transition-transform" />
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>

            {allStepsCompleted && (
              <div className="mt-4 p-3 bg-success-surface border border-success/20 rounded-lg">
                <p className="text-sm text-success font-medium">
                  Great job! You've completed all the onboarding steps.
                </p>
              </div>
            )}
          </div>

          <Button
            variant="ghost"
            size="sm"
            onClick={handleDismiss}
            className="flex-shrink-0 h-8 w-8 p-0 hover:bg-slate-200/50"
            aria-label="Dismiss onboarding"
          >
            <X className="h-4 w-4 text-slate-500" />
          </Button>
        </div>
      </CardContent>
    </Card>
  );
};
