/**
 * DatasetChatPage - Chat interface scoped to a specific dataset
 *
 * Route: /training/datasets/:datasetId/chat
 * Provides a chat experience with RAG context from the dataset's documents.
 */

import { useState, useCallback, useRef } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { ArrowLeft, Database, AlertCircle, Download } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { LoadingState } from '@/components/ui/loading-state';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { ChatInterface } from '@/components/ChatInterface';
import { ExportDialog } from '@/components/export';
import { useTraining } from '@/hooks/training';
import { useTenant } from '@/providers/FeatureProviders';
import { DatasetChatProvider, useDatasetChat } from '@/contexts/DatasetChatContext';
import { toast } from 'sonner';

function DatasetChatPageInner({ dataset }: { dataset: { id: string; name: string; dataset_version_id?: string } }) {
  const navigate = useNavigate();
  const { selectedTenant } = useTenant();
  const [exportDialogOpen, setExportDialogOpen] = useState(false);
  const datasetContext = useDatasetChat();
  const datasetId = dataset.id;

  const handleExport = useCallback(async (format: 'markdown' | 'json' | 'pdf' | 'evidence-bundle') => {
    // Note: Full session export is available via ChatInterface's built-in export
    // This button provides quick access to export dialog with dataset metadata
    toast.info(`Use the export button in the chat area for full session export (${format})`);
    setExportDialogOpen(false);
  }, []);

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <header className="border-b px-4 py-3 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <Button variant="ghost" size="sm" onClick={() => navigate(-1)}>
            <ArrowLeft className="mr-2 h-4 w-4" />
            Back
          </Button>
          <div className="flex items-center gap-2">
            <Database className="h-5 w-5 text-primary" />
            <span className="font-medium">Chat with: {dataset.name}</span>
          </div>
          <Badge variant="secondary" className="gap-1">
            <Database className="h-3 w-3" />
            Dataset Context
          </Badge>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setExportDialogOpen(true)}
            data-testid="dataset-chat-export"
          >
            <Download className="h-4 w-4 mr-2" />
            Export
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => navigate(`/training/datasets/${datasetId}`)}
          >
            View Dataset Details
          </Button>
        </div>
      </header>

      {/* Chat Interface */}
      <main className="flex-1 overflow-hidden">
        <ChatInterface
          selectedTenant={selectedTenant}
          datasetContext={{
            datasetId: dataset.id,
            datasetName: dataset.name,
            datasetVersionId: dataset.dataset_version_id,
          }}
        />
      </main>

      {/* Export Dialog */}
      <ExportDialog
        open={exportDialogOpen}
        onOpenChange={setExportDialogOpen}
        onExport={handleExport}
        title="Export Dataset Chat"
        determinismState="verified"
        availableFormats={['markdown', 'json']}
      />
    </div>
  );
}

export default function DatasetChatPage() {
  const { datasetId } = useParams<{ datasetId: string }>();
  const navigate = useNavigate();
  const { selectedTenant } = useTenant();

  const {
    data: dataset,
    isLoading,
    error,
    refetch,
  } = useTraining.useDataset(datasetId || '', {
    enabled: !!datasetId,
  });

  if (isLoading) {
    return (
      <div className="h-full flex items-center justify-center">
        <LoadingState message="Loading dataset..." />
      </div>
    );
  }

  if (error || !dataset) {
    return (
      <div className="h-full flex items-center justify-center p-4">
        <ErrorRecovery
          error={(error as Error)?.message || 'Dataset not found'}
          onRetry={() => refetch()}
        />
      </div>
    );
  }

  // Check if dataset is ready for chat (valid status)
  const isReadyForChat = dataset.validation_status === 'valid';

  if (!isReadyForChat) {
    return (
      <div className="h-full flex flex-col">
        <header className="border-b px-4 py-3 flex items-center gap-4">
          <Button variant="ghost" size="sm" onClick={() => navigate(-1)}>
            <ArrowLeft className="mr-2 h-4 w-4" />
            Back
          </Button>
          <div className="flex items-center gap-2">
            <Database className="h-5 w-5 text-muted-foreground" />
            <span className="font-medium">{dataset.name}</span>
          </div>
        </header>
        <div className="flex-1 flex items-center justify-center p-4">
          <div className="text-center max-w-md">
            <AlertCircle className="h-12 w-12 mx-auto mb-4 text-amber-500" />
            <h2 className="text-lg font-semibold mb-2">Dataset Not Ready</h2>
            <p className="text-muted-foreground mb-4">
              This dataset needs to be validated before you can chat with it.
              Current status: <Badge variant="outline">{dataset.validation_status}</Badge>
            </p>
            <Button onClick={() => navigate(`/training/datasets/${datasetId}`)}>
              Go to Dataset Details
            </Button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <DatasetChatProvider
      initialDataset={{
        id: dataset.id,
        name: dataset.name,
        versionId: dataset.dataset_version_id,
      }}
    >
      <DatasetChatPageInner dataset={dataset} />
    </DatasetChatProvider>
  );
}
