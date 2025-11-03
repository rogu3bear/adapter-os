import React, { Suspense, lazy } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Loader2 } from 'lucide-react';
import { Persona, Stage } from '../data/persona-journeys';

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
  const MockComponent = stage.content.mockComponent
    ? mockComponents[stage.content.mockComponent as keyof typeof mockComponents]
    : null;

  return (
    <div className="h-full p-4 bg-background">
      <Card className="h-full">
        <CardHeader className="pb-3">
          <div className="flex items-center space-x-2">
            <persona.icon className="h-5 w-5 text-primary" />
            <div>
              <CardTitle className="text-lg">{persona.name}</CardTitle>
              <p className="text-sm text-muted-foreground">{stage.title}</p>
            </div>
          </div>
        </CardHeader>

        <CardContent className="flex-1 overflow-auto">
          {MockComponent ? (
            <Suspense fallback={<LoadingFallback />}>
              <MockComponent />
            </Suspense>
          ) : (
            <div className="flex items-center justify-center h-full text-muted-foreground">
              <div className="text-center">
                <persona.icon className="h-12 w-12 mx-auto mb-4 opacity-50" />
                <h3 className="text-lg font-medium mb-2">Stage Preview</h3>
                <p className="text-sm mb-4">{stage.content.whatAppears}</p>
                <div className="bg-muted p-4 rounded-lg">
                  <p className="text-xs italic">
                    Mock UI component coming soon...
                  </p>
                </div>
              </div>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
