import FeatureLayout from '@/layout/FeatureLayout';
import { WorkflowWizard } from '@/components/WorkflowWizard';
import { DensityProvider } from '@/contexts/DensityContext';
import { Card, CardHeader, CardTitle, CardDescription, CardFooter } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Link } from 'react-router-dom';

export default function WorkflowPage() {
  const buildActions = [
    {
      title: 'Create adapter',
      description: 'Register a new adapter or import an .aos package.',
      to: '/adapters#register',
      cta: 'Open adapters',
    },
    {
      title: 'Start training',
      description: 'Kick off a job or resume drafts from the training hub.',
      to: '/training#jobs',
      cta: 'Go to training',
    },
    {
      title: 'Configure routing',
      description: 'Tune adapter selection and routing guardrails.',
      to: '/router-config',
      cta: 'Open router config',
    },
  ];

  return (
    <DensityProvider pageKey="workflow">
      <FeatureLayout
        title="Onboarding"
        description="Complete the onboarding checklist to finish setup"
        brief="Connect a model, run a probe, and verify evidence."
      >
        <div className="space-y-6">
          <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
            {buildActions.map(action => (
              <Card key={action.title} className="h-full">
                <CardHeader>
                  <CardTitle>{action.title}</CardTitle>
                  <CardDescription>{action.description}</CardDescription>
                </CardHeader>
                <CardFooter>
                  <Button asChild variant="default">
                    <Link to={action.to}>{action.cta}</Link>
                  </Button>
                </CardFooter>
              </Card>
            ))}
          </div>
          <WorkflowWizard />
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
