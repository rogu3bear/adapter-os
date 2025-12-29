import PageWrapper from '@/layout/PageWrapper';
import { TestingPage as TestingPageComponent } from '@/pages/Testing/TestingDetailPage';
import { Card, CardDescription, CardFooter, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Link } from 'react-router-dom';
import { buildReplayLink } from '@/utils/navLinks';

export default function TestingPage() {
  const verifyActions = [
    {
      title: 'Run test suite',
      description: 'Execute regression tests and view results.',
      to: '/testing',
    },
    {
      title: 'Review golden runs',
      description: 'Inspect golden baselines and approvals.',
      to: '/golden',
    },
    {
      title: 'Inspect replay history',
      description: 'Trace inference runs with routing evidence.',
      to: '/replay',
    },
  ];

  return (
    <PageWrapper pageKey="testing" title="Testing" description="Compare against golden baselines">
      <div className="flex justify-end mb-4">
        <Button asChild variant="outline" size="sm">
          <Link to={buildReplayLink('runs')}>View replays from recent tests</Link>
        </Button>
      </div>
      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3 mb-6">
        {verifyActions.map(action => (
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
      <TestingPageComponent />
    </PageWrapper>
  );
}

