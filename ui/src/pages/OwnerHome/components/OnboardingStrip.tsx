import React, { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Rocket, Upload, Box, Play, X, ChevronRight } from 'lucide-react';

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
    path: '/base-models',
  },
  {
    id: 'register-adapter',
    title: 'Register your first adapter',
    description: 'Upload or create a custom LoRA adapter to enhance your model',
    icon: Box,
    action: 'Add Adapter',
    path: '/adapters/new',
  },
  {
    id: 'run-inference',
    title: 'Run a demo inference',
    description: 'Test your setup with a sample prompt and see the results',
    icon: Play,
    action: 'Try Inference',
    path: '/inference',
  },
];

export const OnboardingStrip: React.FC = () => {
  const navigate = useNavigate();
  const [dismissed, setDismissed] = useState<boolean>(false);
  const [completedSteps, setCompletedSteps] = useState<Set<string>>(new Set());

  useEffect(() => {
    const isDismissed = localStorage.getItem(ONBOARDING_STORAGE_KEY);
    if (isDismissed === 'true') {
      setDismissed(true);
    }

    // Check localStorage for completed steps
    const completed = new Set<string>();
    onboardingSteps.forEach(step => {
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

  if (dismissed) {
    return null;
  }

  const allStepsCompleted = onboardingSteps.every(step => completedSteps.has(step.id));

  return (
    <Card className="border-blue-200 bg-gradient-to-r from-blue-50 to-indigo-50 shadow-sm">
      <CardContent className="p-6">
        <div className="flex items-start justify-between gap-4">
          <div className="flex-1">
            <div className="flex items-center gap-3 mb-4">
              <div className="p-2 bg-blue-100 rounded-lg">
                <Rocket className="h-6 w-6 text-blue-600" />
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
                    className="flex items-start gap-3 p-4 bg-white rounded-lg border border-slate-200 hover:border-blue-300 transition-colors"
                  >
                    <div className="flex-shrink-0 mt-1">
                      <button
                        onClick={() => handleStepToggle(step.id)}
                        className={`w-5 h-5 rounded border-2 flex items-center justify-center transition-all ${
                          isCompleted
                            ? 'bg-blue-600 border-blue-600'
                            : 'border-slate-300 hover:border-blue-400'
                        }`}
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
                        <div className="p-1.5 bg-blue-50 rounded">
                          <StepIcon className="h-4 w-4 text-blue-600" />
                        </div>
                        <span className="text-xs font-medium text-slate-500">
                          Step {index + 1}
                        </span>
                      </div>
                      <h4
                        className={`text-sm font-semibold mb-1 ${
                          isCompleted ? 'text-slate-500 line-through' : 'text-slate-900'
                        }`}
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
              <div className="mt-4 p-3 bg-green-50 border border-green-200 rounded-lg">
                <p className="text-sm text-green-800 font-medium">
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
