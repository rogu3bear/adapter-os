/**
 * Dashboard Training Wizard Card Component
 *
 * Quick-start card for the guided training wizard.
 */

import React, { memo } from 'react';
import { Link } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { buildTrainingOverviewLink, buildTrainingDatasetsLink } from '@/utils/navLinks';

/**
 * Training wizard quick-start card for the dashboard.
 *
 * Provides quick access to the guided training workflow and
 * advanced dataset tools.
 */
export const DashboardTrainingWizardCard = memo(function DashboardTrainingWizardCard() {
  return (
    <SectionErrorBoundary sectionName="Training Wizard">
      <Card className="border-primary/40">
        <CardHeader>
          <CardTitle>Training Wizard</CardTitle>
          <p className="text-sm text-muted-foreground">
            Guided: upload or pick a dataset, auto-validate, then start training.
          </p>
        </CardHeader>
        <CardContent className="space-y-3">
          <p className="text-xs text-muted-foreground">
            Best for the common path. For complex datasets, jump to advanced tools.
          </p>
          <div className="flex flex-wrap gap-2">
            <Button asChild>
              <Link to={buildTrainingOverviewLink()} state={{ openTrainingWizard: true }}>
                Start Training Wizard
              </Link>
            </Button>
            <Button variant="outline" asChild>
              <Link to={buildTrainingDatasetsLink()} state={{ openUpload: true }}>
                Advanced dataset tools
              </Link>
            </Button>
          </div>
        </CardContent>
      </Card>
    </SectionErrorBoundary>
  );
});
