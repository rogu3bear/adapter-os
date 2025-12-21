import React, { Suspense, lazy } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Alert, AlertDescription } from './ui/alert';
import { Loader2, ExternalLink, BookOpen } from 'lucide-react';
import { Persona, Stage } from '@/data/persona-journeys';
import { useNavigate } from 'react-router-dom';

// Lazy load mock components
const mockComponents = {
  // ML Engineer stages
  'MLEngineerTrainingSetup': lazy(() => import('./persona-stages/MLEngineerTrainingSetup')),
  'MLEngineerRegistryBrowser': lazy(() => import('./persona-stages/MLEngineerRegistryBrowser')),
  'MLEngineerTrainingMetrics': lazy(() => import('./persona-stages/MLEngineerTrainingMetrics')),
  'MLEngineerInferenceTest': lazy(() => import('./persona-stages/MLEngineerInferenceTest')),

  // DevOps stages
  'DevOpsServerConfig': lazy(() => import('./persona-stages/DevOpsServerConfig')),
  'DevOpsResourceDashboard': lazy(() => import('./persona-stages/DevOpsResourceDashboard')),
  'DevOpsCIDCPanel': lazy(() => import('./persona-stages/DevOpsCIDCPanel')),
  'DevOpsMonitoringDashboard': lazy(() => import('./persona-stages/DevOpsMonitoringDashboard')),

  // App Developer stages
  'AppDevAPIDocs': lazy(() => import('./persona-stages/AppDevAPIDocs')),
  'AppDevSDKManager': lazy(() => import('./persona-stages/AppDevSDKManager')),
  'AppDevTestConsole': lazy(() => import('./persona-stages/AppDevTestConsole')),
  'AppDevPerformancePanel': lazy(() => import('./persona-stages/AppDevPerformancePanel')),

  // Security stages
  'SecurityPolicyEditor': lazy(() => import('./persona-stages/SecurityPolicyEditor')),
  'SecurityAuditTrail': lazy(() => import('./persona-stages/SecurityAuditTrail')),
  'SecurityIsolationTester': lazy(() => import('./persona-stages/SecurityIsolationTester')),
  'SecurityThreatDashboard': lazy(() => import('./persona-stages/SecurityThreatDashboard')),

  // Data Scientist stages
  'DataScientistExperimentTracker': lazy(() => import('./persona-stages/DataScientistExperimentTracker')),
  'DataScientistDatasetManager': lazy(() => import('./persona-stages/DataScientistDatasetManager')),
  'DataScientistEvaluationUI': lazy(() => import('./persona-stages/DataScientistEvaluationUI')),
  'DataScientistCollaborationHub': lazy(() => import('./persona-stages/DataScientistCollaborationHub')),

  // Product Manager stages
  'ProductManagerUsageAnalytics': lazy(() => import('./persona-stages/ProductManagerUsageAnalytics')),
  'ProductManagerPerformanceOverview': lazy(() => import('./persona-stages/ProductManagerPerformanceOverview')),
  'ProductManagerConfigPortal': lazy(() => import('./persona-stages/ProductManagerConfigPortal')),
  'ProductManagerFeedbackHub': lazy(() => import('./persona-stages/ProductManagerFeedbackHub')),
};

interface StageViewerProps {
  persona: Persona;
  stage: Stage;
}

function LoadingFallback() {
  return (
    <div className="flex items-center justify-center h-full">
      <div className="flex items-center space-x-2 text-muted-foreground">
        <Loader2 className="h-4 w-4 animate-spin" />
        <span>Loading stage preview...</span>
      </div>
    </div>
  );
}

export function StageViewer({ persona, stage }: StageViewerProps) {
  const navigate = useNavigate();
  const MockComponent = stage.content.mockComponent
    ? mockComponents[stage.content.mockComponent as keyof typeof mockComponents]
    : null;

  const hasRealPage = !!stage.content.route;
  const hasMentalModelExplanation = !!stage.content.mentalModelExplanation;

  return (
    <div className="h-full p-4 bg-background">
      <Card className="h-full">
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-2">
              <persona.icon className="h-5 w-5 text-primary" />
              <div>
                <CardTitle className="text-lg">{persona.name}</CardTitle>
                <p className="text-sm text-muted-foreground">{stage.title}</p>
              </div>
            </div>
            {hasRealPage && (
              <Button
                size="sm"
                onClick={() => navigate(stage.content.route!)}
              >
                <ExternalLink className="h-4 w-4 mr-1" />
                Open Page
              </Button>
            )}
          </div>
        </CardHeader>

        <CardContent className="space-y-4">
          {/* What Appears */}
          <div>
            <h4 className="text-sm font-semibold mb-1">What appears</h4>
            <p className="text-sm text-muted-foreground">{stage.content.whatAppears}</p>
          </div>

          {/* Why */}
          <div>
            <h4 className="text-sm font-semibold mb-1">Why this matters</h4>
            <p className="text-sm text-muted-foreground">{stage.content.why}</p>
          </div>

          {/* Context */}
          <div>
            <h4 className="text-sm font-semibold mb-1">Context</h4>
            <p className="text-sm text-muted-foreground">{stage.content.context}</p>
          </div>

          {/* Mental Model Explanation */}
          {hasMentalModelExplanation && (
            <Alert>
              <BookOpen className="h-4 w-4" />
              <AlertDescription>
                <div className="space-y-1">
                  <p className="font-semibold text-sm">How this relates to the mental model</p>
                  <p className="text-sm">{stage.content.mentalModelExplanation}</p>
                </div>
              </AlertDescription>
            </Alert>
          )}

          {/* Mock Component (if available) */}
          {MockComponent && (
            <div className="border rounded-lg p-4 bg-muted/30">
              <Suspense fallback={<LoadingFallback />}>
                <MockComponent />
              </Suspense>
            </div>
          )}

          {/* Call to Action */}
          {hasRealPage && (
            <div className="pt-2">
              <Button
                variant="default"
                size="lg"
                className="w-full"
                onClick={() => navigate(stage.content.route!)}
              >
                Go to {stage.title} Page
                <ExternalLink className="h-4 w-4 ml-2" />
              </Button>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
