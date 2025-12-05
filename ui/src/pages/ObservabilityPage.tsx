import FeatureLayout from '@/layout/FeatureLayout';
import { ObservabilityDashboard } from '@/components/ObservabilityDashboard';
import { DensityProvider } from '@/contexts/DensityContext';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { Card, CardDescription, CardFooter, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Link } from 'react-router-dom';

export default function ObservabilityPage() {
  const observeActions = [
    {
      title: 'Metrics',
      description: 'View performance and saturation dashboards.',
      to: '/metrics',
    },
    {
      title: 'System',
      description: 'Inspect nodes, workers, and memory footprint.',
      to: '/system',
    },
    {
      title: 'Telemetry',
      description: 'Audit event history and drill into traces.',
      to: '/telemetry',
    },
  ];

  return (
    <DensityProvider pageKey="observability">
      <FeatureLayout
        title="Monitoring"
        description="System health, telemetry, and routing signals"
      >
        <div className="flex flex-wrap gap-3 mb-4 text-sm">
          <Link to="/metrics" className="underline underline-offset-4">Metrics</Link>
          <Link to="/system" className="underline underline-offset-4">System</Link>
          <Link to="/telemetry" className="underline underline-offset-4">Telemetry</Link>
        </div>
        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3 mb-6">
          {observeActions.map(action => (
            <Card key={action.title} className="h-full">
              <CardHeader>
                <CardTitle>{action.title}</CardTitle>
                <CardDescription>{action.description}</CardDescription>
              </CardHeader>
              <CardFooter>
                <Button asChild variant="secondary" size="sm">
                  <Link to={action.to}>Open</Link>
                </Button>
              </CardFooter>
            </Card>
          ))}
        </div>
        <SectionErrorBoundary sectionName="Observability">
          <ObservabilityDashboard />
        </SectionErrorBoundary>
      </FeatureLayout>
    </DensityProvider>
  );
}

