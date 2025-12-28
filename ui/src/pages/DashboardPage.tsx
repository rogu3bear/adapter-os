import { useMemo } from 'react';
import { Link } from 'react-router-dom';
import PageWrapper from '@/layout/PageWrapper';
import { useTenant } from '@/providers/FeatureProviders';
import { useWorkspaces } from '@/hooks/workspace/useWorkspaces';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Progress } from '@/components/ui/progress';
import {
  buildBaseModelsLink,
  buildChatLink,
  buildDocumentsLink,
  buildTrainingOverviewLink,
  buildWorkspacesLink,
} from '@/utils/navLinks';
import { Building2, Database, FileUp, MessageSquare, Zap } from 'lucide-react';

type StepStatus = 'done' | 'active' | 'pending';

interface Step {
  id: string;
  title: string;
  description: string;
  to: string;
  icon: React.ComponentType<{ className?: string }>;
  status: StepStatus;
}

export default function DashboardPage() {
  const { selectedTenant } = useTenant();
  const { userWorkspaces, workspaces } = useWorkspaces();

  const workspaceName = useMemo(() => {
    if (!selectedTenant) return '';
    const all = [...userWorkspaces, ...workspaces];
    return all.find(ws => ws.id === selectedTenant)?.name || selectedTenant;
  }, [selectedTenant, userWorkspaces, workspaces]);

  const baseSteps: Omit<Step, 'status'>[] = [
    {
      id: 'workspace',
      title: 'Select Workspace',
      description: 'Scope every action to the right workspace.',
      to: buildWorkspacesLink(),
      icon: Building2,
    },
    {
      id: 'base-model',
      title: 'Load Base Model',
      description: 'Review or assign the base model for this workspace.',
      to: buildBaseModelsLink(),
      icon: Database,
    },
    {
      id: 'data',
      title: 'Upload Data',
      description: 'Add documents or datasets that will feed tuning.',
      to: buildDocumentsLink(),
      icon: FileUp,
    },
    {
      id: 'tune',
      title: 'Start Tune',
      description: 'Configure a training job and push a run live.',
      to: buildTrainingOverviewLink(),
      icon: Zap,
    },
    {
      id: 'chat',
      title: 'Chat',
      description: 'Validate the tuned model with real conversations.',
      to: buildChatLink(),
      icon: MessageSquare,
    },
  ];

  const completedCount = selectedTenant ? 1 : 0;
  const steps: Step[] = baseSteps.map((step, index) => {
    const status: StepStatus =
      index < completedCount ? 'done' : index === completedCount ? 'active' : 'pending';
    return { ...step, status };
  });

  const progressValue = Math.round((completedCount / steps.length) * 100);

  return (
    <PageWrapper
      pageKey="mvp-home"
      title="Home"
      description="One path to get a workspace online."
      maxWidth="xl"
    >
      <div className="grid gap-4 lg:grid-cols-[2fr,1fr]">
        <Card className="lg:col-span-1">
          <CardHeader className="flex flex-row items-center justify-between">
            <div>
              <CardTitle className="text-lg">Workspace status</CardTitle>
              <CardDescription>
                {workspaceName ? 'Workspace ready for next step.' : 'Pick a workspace to begin.'}
              </CardDescription>
            </div>
            <Badge variant={workspaceName ? 'default' : 'outline'}>
              {workspaceName ? 'Selected' : 'Not selected'}
            </Badge>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex items-center gap-2">
              <Progress value={progressValue} aria-label="MVP flow progress" />
              <span className="text-sm text-muted-foreground w-12 text-right">{progressValue}%</span>
            </div>
            <div className="rounded-md border bg-muted/30 p-3 text-sm">
              <div className="font-medium">Active workspace</div>
              <div className="text-muted-foreground">
                {workspaceName || 'No workspace selected yet.'}
              </div>
            </div>
            <Button asChild>
              <Link to={buildWorkspacesLink()}>
                {workspaceName ? 'Switch workspace' : 'Select workspace'}
              </Link>
            </Button>
          </CardContent>
        </Card>

        <Card className="lg:col-span-2">
          <CardHeader>
            <CardTitle>Get started</CardTitle>
            <CardDescription>Follow the steps in order; each link opens the destination page.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {steps.map(step => (
              <div
                key={step.id}
                className="flex flex-col gap-2 rounded-lg border p-3 md:flex-row md:items-center md:justify-between"
              >
                <div className="flex items-center gap-3">
                  <div className="rounded-md bg-muted p-2">
                    <step.icon className="h-5 w-5" />
                  </div>
                  <div>
                    <div className="flex items-center gap-2">
                      <span className="font-semibold">{step.title}</span>
                      <Badge variant={step.status === 'done' ? 'secondary' : 'outline'}>
                        {step.status === 'done' ? 'Done' : step.status === 'active' ? 'Next' : 'Pending'}
                      </Badge>
                    </div>
                    <p className="text-sm text-muted-foreground">{step.description}</p>
                  </div>
                </div>
                <Button asChild variant="outline" size="sm">
                  <Link to={step.to}>Open</Link>
                </Button>
              </div>
            ))}
          </CardContent>
        </Card>
      </div>
    </PageWrapper>
  );
}
