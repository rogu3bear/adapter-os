import React, { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Progress } from './ui/progress';
import { Alert, AlertDescription } from './ui/alert';
import { Checkbox } from './ui/checkbox';
import { ArrowRight, CheckCircle, Circle, Shield, Zap, Activity } from 'lucide-react';
import { useAuth, useTenant } from '@/layout/LayoutProvider';
import { logger, toError } from '@/utils/logger';

type StepStatus = 'not_started' | 'in_progress' | 'done';

interface ChecklistStep {
  id: string;
  title: string;
  description: string;
  primaryLink: { label: string; href: string; disabled?: boolean };
  secondaryLink?: { label: string; href: string; disabled?: boolean };
}

const CHECKLIST_STEPS: ChecklistStep[] = [
  {
    id: 'connect',
    title: 'Connect base model & register adapter',
    description: 'Load a base model and register your first adapter.',
    primaryLink: { label: 'Base models', href: '/base-models' },
    secondaryLink: { label: 'Create adapter', href: '/create-adapter' },
  },
  {
    id: 'probe',
    title: 'Run a sample inference & chat sanity probe',
    description: 'Send a test request and open the chat sandbox.',
    primaryLink: { label: 'Inference', href: '/inference' },
    secondaryLink: { label: 'Chat', href: '/chat' },
  },
  {
    id: 'verify',
    title: 'Verify evidence',
    description: 'Inspect telemetry and replay a recent run.',
    primaryLink: { label: 'Telemetry', href: '/telemetry' },
    secondaryLink: { label: 'Replay', href: '/replay' },
  },
];

const STATUS_ORDER: StepStatus[] = ['not_started', 'in_progress', 'done'];
const STORAGE_KEY_PREFIX = 'aos_workflow_checklist';

const statusLabel = (status: StepStatus) => {
  switch (status) {
    case 'in_progress':
      return 'In progress';
    case 'done':
      return 'Done';
    default:
      return 'Not started';
  }
};

function getStorageKey(userId?: string, tenantId?: string) {
  return `${STORAGE_KEY_PREFIX}_${tenantId || 'default'}_${userId || 'anon'}`;
}

export function WorkflowWizard() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const navigate = useNavigate();

  const storageKey = useMemo(
    () => getStorageKey(user?.id, selectedTenant || user?.tenant_id),
    [user?.id, user?.tenant_id, selectedTenant]
  );

  const [statuses, setStatuses] = useState<Record<string, StepStatus>>({});

  useEffect(() => {
    try {
      const saved = localStorage.getItem(storageKey);
      if (saved) {
        const parsed = JSON.parse(saved);
        setStatuses(parsed);
      } else {
        const initial: Record<string, StepStatus> = {};
        CHECKLIST_STEPS.forEach(step => {
          initial[step.id] = 'not_started';
        });
        setStatuses(initial);
      }
    } catch (err) {
      logger.error('Failed to load workflow checklist', { component: 'WorkflowWizard' }, toError(err));
    }
  }, [storageKey]);

  const persistStatuses = (next: Record<string, StepStatus>) => {
    try {
      localStorage.setItem(storageKey, JSON.stringify(next));
    } catch (err) {
      logger.error('Failed to save workflow checklist', { component: 'WorkflowWizard' }, toError(err));
    }
  };

  const updateStatus = (id: string, status: StepStatus) => {
    setStatuses(prev => {
      const next = { ...prev, [id]: status };
      persistStatuses(next);
      return next;
    });
  };

  const completionCount = CHECKLIST_STEPS.filter(s => statuses[s.id] === 'done').length;
  const completionPct = (completionCount / CHECKLIST_STEPS.length) * 100;

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle>Onboarding checklist</CardTitle>
          <CardDescription>Complete these steps to finish initial setup.</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="space-y-3">
            <div className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground">Progress</span>
              <span className="font-medium">
                {completionCount} / {CHECKLIST_STEPS.length} done
              </span>
            </div>
            <Progress value={completionPct} className="h-2" />
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Required steps</CardTitle>
          <CardDescription>Mark each step as you advance.</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="space-y-3">
            {CHECKLIST_STEPS.map(step => {
              const status = statuses[step.id] || 'not_started';
              const isDone = status === 'done';
              return (
                <div
                  key={step.id}
                  className={`flex flex-col md:flex-row md:items-center gap-3 p-4 rounded-lg border ${
                    isDone ? 'bg-green-50 border-green-200' : 'bg-muted/30'
                  }`}
                >
                  <div className="flex items-start gap-3 flex-1">
                    <div className="mt-1">
                      {isDone ? (
                        <CheckCircle className="h-5 w-5 text-green-600" />
                      ) : (
                        <Circle className="h-5 w-5 text-muted-foreground" />
                      )}
                    </div>
                    <div className="space-y-1">
                      <div className="flex items-center gap-2">
                        <h4 className="font-medium text-sm">{step.title}</h4>
                        <Badge variant={isDone ? 'default' : 'secondary'} className="text-xs">
                          {statusLabel(status)}
                        </Badge>
                      </div>
                      <p className="text-sm text-muted-foreground">{step.description}</p>
                      <div className="flex flex-wrap gap-2 pt-1">
                        <Button
                          size="sm"
                          onClick={() => navigate(step.primaryLink.href)}
                          disabled={step.primaryLink.disabled}
                        >
                          {step.primaryLink.label}
                          <ArrowRight className="h-4 w-4 ml-1" />
                        </Button>
                        {step.secondaryLink && (
                          <Button
                            size="sm"
                            variant="outline"
                            onClick={() => navigate(step.secondaryLink.href)}
                            disabled={step.secondaryLink.disabled}
                          >
                            {step.secondaryLink.label}
                          </Button>
                        )}
                      </div>
                    </div>
                  </div>
                  <div className="flex items-center gap-3">
                    <div className="flex flex-col gap-1 text-xs text-muted-foreground">
                      <span>Status</span>
                      <div className="flex items-center gap-2">
                        {STATUS_ORDER.map(s => (
                          <label key={s} className="flex items-center gap-1 cursor-pointer">
                            <Checkbox
                              checked={status === s}
                              onCheckedChange={() => updateStatus(step.id, s)}
                              aria-label={`Mark ${step.title} as ${statusLabel(s)}`}
                            />
                            <span className={status === s ? 'text-foreground font-medium' : undefined}>
                              {statusLabel(s)}
                            </span>
                          </label>
                        ))}
                      </div>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Next hardening steps</CardTitle>
          <CardDescription>Continue strengthening your deployment.</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-wrap gap-2">
          <Button variant="outline" size="sm" onClick={() => navigate('/promotion')}>
            <Zap className="h-4 w-4 mr-2" />
            Promotion
          </Button>
          <Button variant="outline" size="sm" onClick={() => navigate('/security/policies')}>
            <Shield className="h-4 w-4 mr-2" />
            Policies
          </Button>
          <Button variant="outline" size="sm" onClick={() => navigate('/routing')}>
            <Activity className="h-4 w-4 mr-2" />
            Routing
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}
