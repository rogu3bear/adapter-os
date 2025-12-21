import React from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import PageWrapper from '@/layout/PageWrapper';
import { TrainingWizard } from '@/components/TrainingWizard';
import { buildTrainingJobDetailLink } from '@/utils/navLinks';

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
    navigate(buildTrainingJobDetailLink(jobId));
  };

  const handleCancel = () => {
    // Go back to previous page or dashboard
    navigate(-1);
  };

  return (
    <PageWrapper
      pageKey="create-adapter"
      title="Create Adapter"
      description="Train a custom LoRA adapter in 3 simple steps"
    >
      <TrainingWizard
        onComplete={handleComplete}
        onCancel={handleCancel}
        initialDatasetId={preselectedDatasetId}
        lockDatasetId={!!preselectedDatasetId}
        isStandalonePage
        hideSimpleModeToggle
      />
    </PageWrapper>
  );
}

