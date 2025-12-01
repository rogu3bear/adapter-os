import React from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import FeatureLayout from '@/layout/FeatureLayout';
import { TrainingWizard } from '@/components/TrainingWizard';
import { DensityProvider } from '@/contexts/DensityContext';
import { PageErrorsProvider } from '@/components/ui/page-error-boundary';

/**
 * Create Adapter Page
 * 
 * Full-page rendering of the TrainingWizard component for creating custom LoRA adapters.
 * Provides a streamlined 3-step flow: Upload/Select Data → Configure Parameters → Review & Start
 * 
 * This is the primary entry point for adapter training, promoted from modal-only to first-class page.
 */
export default function CreateAdapterPage() {
  const navigate = useNavigate();
  const location = useLocation();
  
  // Extract preselected dataset ID from navigation state (e.g., from GuidedFlowPage)
  const preselectedDatasetId = (location.state as { preselectedDatasetId?: string })?.preselectedDatasetId;

  const handleComplete = (jobId: string) => {
    // Navigate to training job detail page to monitor progress
    navigate(`/training/jobs/${jobId}`);
  };

  const handleCancel = () => {
    // Go back to previous page or dashboard
    navigate(-1);
  };

  return (
    <DensityProvider pageKey="create-adapter">
      <FeatureLayout 
        title="Create Adapter" 
        description="Train a custom LoRA adapter in 3 simple steps"
      >
        <PageErrorsProvider>
          <TrainingWizard
            onComplete={handleComplete}
            onCancel={handleCancel}
            initialDatasetId={preselectedDatasetId}
            lockDatasetId={!!preselectedDatasetId}
            isStandalonePage
            hideSimpleModeToggle
          />
        </PageErrorsProvider>
      </FeatureLayout>
    </DensityProvider>
  );
}

