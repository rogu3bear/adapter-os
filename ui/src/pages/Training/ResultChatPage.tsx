/**
 * ResultChatPage - Chat interface for training job results
 *
 * Route: /training/jobs/:jobId/chat
 *
 * Shows chat with:
 * - Trained adapter for inference (via stack)
 * - Source dataset for citations (via dataset_version_id)
 * - Header chips showing both adapter and dataset context
 */

import { useParams, useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import {
  ArrowLeft,
  Database,
  Layers,
  AlertCircle,
  MessageSquare,
} from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { LoadingState } from '@/components/ui/loading-state';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { ChatInterface } from '@/components/ChatInterface';
import { useTenant } from '@/providers/FeatureProviders';
import { DatasetChatProvider } from '@/contexts/DatasetChatContext';
import apiClient from '@/api/client';

/**
 * Header component with adapter and dataset chips
 *
 * Note: Export functionality is handled by ChatInterface's built-in export button.
 */
function ResultChatHeader({
  adapterName,
  adapterVersionId,
  datasetName,
  datasetVersionId,
  onBack,
  onViewJob,
}: {
  adapterName?: string;
  adapterVersionId?: string;
  datasetName?: string;
  datasetVersionId?: string;
  onBack: () => void;
  onViewJob: () => void;
}) {
  return (
    <header className="border-b px-4 py-3 flex items-center justify-between">
      <div className="flex items-center gap-4">
        <Button variant="ghost" size="sm" onClick={onBack}>
          <ArrowLeft className="mr-2 h-4 w-4" />
          Back
        </Button>
        <div className="flex items-center gap-2">
          <MessageSquare className="h-5 w-5 text-primary" />
          <span className="font-medium">Result Chat</span>
        </div>

        {/* Context chips */}
        <div className="flex items-center gap-2">
          {adapterName && (
            <Badge variant="secondary" className="gap-1.5 px-2 py-1">
              <Layers className="h-3.5 w-3.5" />
              <span className="text-xs font-medium">
                Adapter: {adapterName}
                {adapterVersionId && `@${adapterVersionId.slice(0, 8)}`}
              </span>
            </Badge>
          )}
          {datasetName && (
            <Badge variant="outline" className="gap-1.5 px-2 py-1">
              <Database className="h-3.5 w-3.5" />
              <span className="text-xs font-medium">
                Dataset: {datasetName}
                {datasetVersionId && `#${datasetVersionId.slice(0, 8)}`}
              </span>
            </Badge>
          )}
        </div>
      </div>

      <Button variant="outline" size="sm" onClick={onViewJob}>
        View Job Details
      </Button>
    </header>
  );
}

/**
 * Not ready state component
 */
function NotReadyState({
  status,
  onViewJob,
}: {
  status: string;
  onViewJob: () => void;
}) {
  return (
    <div className="h-full flex flex-col">
      <header className="border-b px-4 py-3 flex items-center gap-4">
        <MessageSquare className="h-5 w-5 text-muted-foreground" />
        <span className="font-medium">Result Chat</span>
      </header>
      <div className="flex-1 flex items-center justify-center p-4">
        <div className="text-center max-w-md">
          <AlertCircle className="h-12 w-12 mx-auto mb-4 text-amber-500" />
          <h2 className="text-lg font-semibold mb-2">Chat Not Ready</h2>
          <p className="text-muted-foreground mb-4">
            This training job hasn't completed yet. Chat will be available once training finishes
            and an adapter is created.
            <br />
            <Badge variant="outline" className="mt-2">
              Status: {status}
            </Badge>
          </p>
          <Button onClick={onViewJob}>View Job Progress</Button>
        </div>
      </div>
    </div>
  );
}

/**
 * Inner component with chat interface
 */
function ResultChatPageInner({
  jobId,
  adapterName,
  adapterVersionId,
  datasetId,
  datasetName,
  datasetVersionId,
  stackId,
}: {
  jobId: string;
  adapterName?: string;
  adapterVersionId?: string;
  datasetId?: string;
  datasetName?: string;
  datasetVersionId?: string;
  stackId: string;
}) {
  const navigate = useNavigate();
  const { selectedTenant } = useTenant();

  return (
    <div className="h-full flex flex-col">
      <ResultChatHeader
        adapterName={adapterName}
        adapterVersionId={adapterVersionId}
        datasetName={datasetName}
        datasetVersionId={datasetVersionId}
        onBack={() => navigate(-1)}
        onViewJob={() => navigate(`/training/jobs/${jobId}`)}
      />

      <main className="flex-1 overflow-hidden">
        <ChatInterface
          selectedTenant={selectedTenant}
          initialStackId={stackId}
          datasetContext={
            datasetId
              ? {
                  datasetId,
                  datasetName: datasetName || 'Training Dataset',
                  datasetVersionId,
                }
              : undefined
          }
        />
      </main>
    </div>
  );
}

/**
 * Main ResultChatPage component
 */
export default function ResultChatPage() {
  const { jobId } = useParams<{ jobId: string }>();
  const navigate = useNavigate();

  if (!jobId) {
    return (
      <div className="h-full flex items-center justify-center p-4">
        <ErrorRecovery
          error="Missing training job ID."
          onRetry={() => navigate('/training/jobs')}
        />
      </div>
    );
  }

  // Fetch training job for metadata
  const {
    data: job,
    isLoading: isLoadingJob,
    error: jobError,
    refetch: refetchJob,
  } = useQuery({
    queryKey: ['training-job', jobId],
    queryFn: () => apiClient.getTrainingJob(jobId),
  });

  // Fetch chat bootstrap data
  const {
    data: bootstrap,
    isLoading: isLoadingBootstrap,
    error: bootstrapError,
    refetch: refetchBootstrap,
  } = useQuery({
    queryKey: ['chat-bootstrap', jobId],
    queryFn: () => apiClient.getChatBootstrap(jobId),
  });

  const isLoading = isLoadingJob || isLoadingBootstrap;
  const error = jobError || bootstrapError;

  if (isLoading) {
    return (
      <div className="h-full flex items-center justify-center">
        <LoadingState message="Preparing result chat..." />
      </div>
    );
  }

  if (error) {
    return (
      <div className="h-full flex items-center justify-center p-4">
        <ErrorRecovery
          error={(error as Error)?.message || 'Failed to load chat'}
          onRetry={() => Promise.allSettled([refetchJob(), refetchBootstrap()])}
        />
      </div>
    );
  }

  // Check if chat is ready
  if (!bootstrap?.ready || !bootstrap?.stack_id) {
    return (
      <NotReadyState
        status={bootstrap?.status || job?.status || 'unknown'}
        onViewJob={() => navigate(`/training/jobs/${jobId}`)}
      />
    );
  }

  // Wrap in DatasetChatProvider if we have dataset context
  const content = (
    <ResultChatPageInner
      jobId={jobId}
      adapterName={job?.adapter_name}
      adapterVersionId={bootstrap.adapter_version_id}
      datasetId={bootstrap.dataset_id}
      datasetName={bootstrap.dataset_name || job?.adapter_name}
      datasetVersionId={bootstrap.dataset_version_id}
      stackId={bootstrap.stack_id}
    />
  );

  if (bootstrap.dataset_id) {
    return (
      <DatasetChatProvider
        initialDataset={{
          id: bootstrap.dataset_id,
          name: bootstrap.dataset_name || job?.adapter_name || 'Training Dataset',
          versionId: bootstrap.dataset_version_id,
        }}
      >
        {content}
      </DatasetChatProvider>
    );
  }

  return content;
}
