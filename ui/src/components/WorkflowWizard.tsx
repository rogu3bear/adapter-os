import React, { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { logger, toError } from '@/utils/logger';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Progress } from './ui/progress';
import { Alert, AlertDescription } from './ui/alert';
import { 
  CheckCircle, 
  Circle, 
  ArrowRight, 
  BookOpen, 
  Rocket,
  Shield,
  Activity,
  FileText,
  Zap
} from 'lucide-react';
import { useAuth } from '@/layout/LayoutProvider';
import type { User, UserRole } from '@/api/types';
import { getRoleGuidance } from '@/data/role-guidance';

interface WorkflowStep {
  id: string;
  title: string;
  description: string;
  icon: any;
  route: string;
  completed: boolean;
}

interface RoleWorkflow {
  title: string;
  description: string;
  icon: any;
  steps: Omit<WorkflowStep, 'completed'>[];
}

const workflowsByRole: Record<UserRole, RoleWorkflow> = {
  Admin: {
    title: 'System Administrator Workflow',
    description: 'Set up and configure the AdapterOS system',
    icon: Shield,
    steps: [
      {
        id: 'system-overview',
        title: 'System Overview',
        description: 'Review system health and infrastructure',
        icon: Activity,
        route: '/dashboard'
      },
      {
        id: 'policies',
        title: 'Configure Policies',
        description: 'Set up security policies and compliance rules',
        icon: Shield,
        route: '/policies'
      },
      {
        id: 'monitoring',
        title: 'Setup Monitoring',
        description: 'Configure alerts and system monitoring',
        icon: Activity,
        route: '/monitoring'
      },
      {
        id: 'telemetry',
        title: 'Review Telemetry',
        description: 'Configure telemetry collection and audit trails',
        icon: FileText,
        route: '/telemetry'
      }
    ]
  },
  Operator: {
    title: 'ML Operations Workflow',
    description: 'Train, test, and deploy ML adapters',
    icon: Zap,
    steps: [
      {
        id: 'dashboard',
        title: 'ML Pipeline Overview',
        description: 'Review ML pipeline status',
        icon: Activity,
        route: '/dashboard'
      },
      {
        id: 'training',
        title: 'Train Adapter',
        description: 'Start training a new adapter',
        icon: Zap,
        route: '/training'
      },
      {
        id: 'testing',
        title: 'Test & Validate',
        description: 'Run tests and validate adapter performance',
        icon: CheckCircle,
        route: '/testing'
      },
      {
        id: 'promotion',
        title: 'Promote Adapter',
        description: 'Promote tested adapter through quality gates',
        icon: ArrowRight,
        route: '/promotion'
      },
      {
        id: 'deploy',
        title: 'Deploy & Manage',
        description: 'Deploy adapter and manage lifecycle',
        icon: Rocket,
        route: '/adapters'
      }
    ]
  },
  SRE: {
    title: 'Site Reliability Workflow',
    description: 'Monitor system health and respond to incidents',
    icon: Activity,
    steps: [
      {
        id: 'monitoring',
        title: 'System Health Dashboard',
        description: 'Review system metrics and alerts',
        icon: Activity,
        route: '/monitoring'
      },
      {
        id: 'dashboard',
        title: 'Resource Overview',
        description: 'Check resource utilization',
        icon: Activity,
        route: '/dashboard'
      },
      {
        id: 'telemetry',
        title: 'Telemetry Analysis',
        description: 'Analyze system telemetry for issues',
        icon: FileText,
        route: '/telemetry'
      },
      {
        id: 'routing',
        title: 'Routing Inspector',
        description: 'Inspect routing decisions and performance',
        icon: ArrowRight,
        route: '/routing'
      }
    ]
  },
  Compliance: {
    title: 'Compliance Officer Workflow',
    description: 'Review policies, audit trails, and compliance status',
    icon: Shield,
    steps: [
      {
        id: 'dashboard',
        title: 'Compliance Dashboard',
        description: 'Review compliance overview',
        icon: Activity,
        route: '/dashboard'
      },
      {
        id: 'policies',
        title: 'Policy Review',
        description: 'Review and manage security policies',
        icon: Shield,
        route: '/policies'
      },
      {
        id: 'audit',
        title: 'Audit Trails',
        description: 'Review system audit trails',
        icon: FileText,
        route: '/audit'
      },
      {
        id: 'telemetry',
        title: 'Telemetry Bundles',
        description: 'Export and verify telemetry data',
        icon: FileText,
        route: '/telemetry'
      }
    ]
  },
  Auditor: {
    title: 'Auditor Workflow',
    description: 'Audit system activity and verify compliance',
    icon: FileText,
    steps: [
      {
        id: 'audit',
        title: 'Audit Trails',
        description: 'Review comprehensive audit trails',
        icon: FileText,
        route: '/audit'
      },
      {
        id: 'telemetry',
        title: 'Verify Telemetry',
        description: 'Verify telemetry bundle signatures',
        icon: Shield,
        route: '/telemetry'
      },
      {
        id: 'replay',
        title: 'Replay Sessions',
        description: 'Review and verify replay sessions',
        icon: ArrowRight,
        route: '/replay'
      },
      {
        id: 'policies',
        title: 'Policy Compliance',
        description: 'Review policy compliance status',
        icon: Shield,
        route: '/policies'
      }
    ]
  },
  Viewer: {
    title: 'Viewer Workflow',
    description: 'Monitor system status and view reports',
    icon: Activity,
    steps: [
      {
        id: 'dashboard',
        title: 'System Overview',
        description: 'View system dashboard',
        icon: Activity,
        route: '/dashboard'
      },
      {
        id: 'adapters',
        title: 'Adapter Status',
        description: 'View adapter status and metrics',
        icon: Rocket,
        route: '/adapters'
      },
      {
        id: 'inference',
        title: 'Try Inference',
        description: 'Test inference playground',
        icon: Zap,
        route: '/inference'
      }
    ]
  }
};

const STORAGE_KEY = 'aos_workflow_progress';

export function WorkflowWizard() {
  const { user } = useAuth();
  const navigate = useNavigate();
  const [completedSteps, setCompletedSteps] = useState<string[]>([]);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    // Load progress from localStorage
    try {
      const saved = localStorage.getItem(STORAGE_KEY);
      if (saved) {
        const data = JSON.parse(saved);
        setCompletedSteps(data.completedSteps || []);
        setDismissed(data.dismissed || false);
      }
    } catch (err) {
      logger.error('Failed to load workflow progress', { component: 'WorkflowWizard', operation: 'loadProgress' }, toError(err));
    }
  }, []);

  const saveProgress = (steps: string[], isDismissed: boolean) => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({
        completedSteps: steps,
        dismissed: isDismissed
      }));
    } catch (err) {
      logger.error('Failed to save workflow progress', { component: 'WorkflowWizard', operation: 'saveProgress' }, toError(err));
    }
  };

  const markStepCompleted = (stepId: string) => {
    const updated = [...new Set([...completedSteps, stepId])];
    setCompletedSteps(updated);
    saveProgress(updated, dismissed);
  };

  const handleDismiss = () => {
    setDismissed(true);
    saveProgress(completedSteps, true);
  };

  const handleReset = () => {
    setCompletedSteps([]);
    setDismissed(false);
    saveProgress([], false);
  };

  if (!user) return null;

  const workflow = workflowsByRole[user.role];
  const roleGuidance = getRoleGuidance(user.role);
  
  const stepsWithCompletion: WorkflowStep[] = workflow.steps.map(step => ({
    ...step,
    completed: completedSteps.includes(step.id)
  }));

  const completionPercentage = (completedSteps.length / workflow.steps.length) * 100;
  const allCompleted = completedSteps.length === workflow.steps.length;

  if (dismissed && !allCompleted) {
    return (
      <Card>
        <CardContent className="pt-6">
          <div className="flex items-center justify-between">
            <p className="text-sm text-muted-foreground">
              Workflow wizard dismissed. You can always resume it later.
            </p>
            <Button variant="outline" onClick={handleReset}>
              Resume Workflow
            </Button>
          </div>
        </CardContent>
      </Card>
    );
  }

  const WorkflowIcon = workflow.icon;

  return (
    <div className="space-y-6">
      {/* Header Card */}
      <Card>
        <CardHeader>
          <div className="flex items-start justify-between">
            <div className="flex items-center gap-3">
              <div className="p-2 bg-primary/10 rounded-lg">
                <WorkflowIcon className="h-6 w-6 text-primary" />
              </div>
              <div>
                <CardTitle>{workflow.title}</CardTitle>
                <CardDescription className="mt-1">
                  {workflow.description}
                </CardDescription>
              </div>
            </div>
            {!allCompleted && (
              <Button variant="ghost" size="sm" onClick={handleDismiss}>
                Dismiss
              </Button>
            )}
          </div>
        </CardHeader>
        <CardContent>
          <div className="space-y-2">
            <div className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground">Progress</span>
              <span className="font-medium">
                {completedSteps.length} / {workflow.steps.length} completed
              </span>
            </div>
            <Progress value={completionPercentage} className="h-2" />
          </div>
        </CardContent>
      </Card>

      {/* Role Guidance */}
      {roleGuidance && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Role Overview</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <p className="text-sm text-muted-foreground">
              {roleGuidance.description}
            </p>
            <div className="space-y-2">
              <p className="text-sm font-medium">Key Capabilities:</p>
              <ul className="text-sm text-muted-foreground space-y-1">
                {roleGuidance.capabilities.slice(0, 4).map((cap, idx) => (
                  <li key={idx} className="flex items-start gap-2">
                    <CheckCircle className="h-4 w-4 text-green-600 mt-0.5 flex-shrink-0" />
                    <span>{cap}</span>
                  </li>
                ))}
              </ul>
            </div>
            {roleGuidance.tips.length > 0 && (
              <Alert>
                <BookOpen className="h-4 w-4" />
                <AlertDescription>
                  <span className="font-medium">Quick Tip: </span>
                  {roleGuidance.tips[0]}
                </AlertDescription>
              </Alert>
            )}
          </CardContent>
        </Card>
      )}

      {/* Workflow Steps */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Guided Workflow</CardTitle>
          <CardDescription>
            Follow these steps to get started with AdapterOS
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="space-y-3">
            {stepsWithCompletion.map((step, idx) => {
              const StepIcon = step.icon;
              return (
                <div
                  key={step.id}
                  className={`flex items-start gap-4 p-4 rounded-lg border transition-colors ${
                    step.completed
                      ? 'bg-green-50 border-green-200'
                      : 'bg-muted/50 border-border hover:bg-muted'
                  }`}
                >
                  <div className="flex-shrink-0 mt-1">
                    {step.completed ? (
                      <CheckCircle className="h-5 w-5 text-green-600" />
                    ) : (
                      <Circle className="h-5 w-5 text-muted-foreground" />
                    )}
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <StepIcon className="h-4 w-4 text-muted-foreground" />
                      <h4 className="font-medium text-sm">{step.title}</h4>
                      {step.completed && (
                        <Badge variant="secondary" className="text-xs">
                          Completed
                        </Badge>
                      )}
                    </div>
                    <p className="text-sm text-muted-foreground">
                      {step.description}
                    </p>
                  </div>
                  <Button
                    variant={step.completed ? 'ghost' : 'default'}
                    size="sm"
                    onClick={() => {
                      navigate(step.route);
                      if (!step.completed) {
                        markStepCompleted(step.id);
                      }
                    }}
                  >
                    {step.completed ? 'Revisit' : 'Start'}
                    <ArrowRight className="h-4 w-4 ml-1" />
                  </Button>
                </div>
              );
            })}
          </div>

          {allCompleted && (
            <Alert className="mt-4 bg-green-50 border-green-200">
              <CheckCircle className="h-4 w-4 text-green-600" />
              <AlertDescription>
                <span className="font-medium text-green-900">
                  Congratulations!
                </span>{' '}
                <span className="text-green-800">
                  You've completed the initial workflow. You can now use AdapterOS freely.
                </span>
              </AlertDescription>
            </Alert>
          )}
        </CardContent>
      </Card>

      {/* Quick Actions */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Quick Actions</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            <Button
              variant="outline"
              className="justify-start"
              onClick={() => navigate('/dashboard')}
            >
              <Activity className="h-4 w-4 mr-2" />
              Go to Dashboard
            </Button>
            <Button
              variant="outline"
              className="justify-start"
              onClick={handleReset}
            >
              <ArrowRight className="h-4 w-4 mr-2" />
              Reset Workflow
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
