import React, { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { FileText, Folder, Calendar, Hash, ExternalLink, Download } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { PDFViewer } from '@/components/documents/PDFViewer';
import { toast } from 'sonner';

interface TrainingDocument {
  doc_id: string;
  doc_name: string;
  doc_hash: string;
  page_count: number;
}

interface TrainingSnapshot {
  id: string;
  adapter_id: string;
  training_job_id: string;
  collection_id: string | null;
  collection_name: string | null;
  documents: TrainingDocument[];
  chunk_manifest_hash: string;
  created_at: string;
}

interface Props {
  adapterId: string;
}

export function TrainingSnapshotPanel({ adapterId }: Props) {
  const navigate = useNavigate();
  const [snapshot, setSnapshot] = useState<TrainingSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [exporting, setExporting] = useState(false);
  const [selectedDocument, setSelectedDocument] = useState<TrainingDocument | null>(null);
  const [isPdfViewerOpen, setIsPdfViewerOpen] = useState(false);

  useEffect(() => {
    fetchSnapshot();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- fetchSnapshot is not stable, only run when adapterId changes
  }, [adapterId]);

  const fetchSnapshot = async () => {
    try {
      const response = await fetch(`/api/v1/adapters/${adapterId}/training-snapshot`);
      if (response.ok) {
        setSnapshot(await response.json());
      } else if (response.status === 404) {
        // No training snapshot available - this is OK
        setSnapshot(null);
      } else {
        toast.error('Failed to fetch training provenance');
      }
    } catch (error) {
      console.error('Failed to fetch training snapshot:', error);
      toast.error('Failed to fetch training provenance');
    } finally {
      setLoading(false);
    }
  };

  const handleViewDocument = (doc: TrainingDocument) => {
    setSelectedDocument(doc);
    setIsPdfViewerOpen(true);
  };

  const handleCollectionClick = () => {
    if (snapshot?.collection_id) {
      navigate(`/training/datasets/${snapshot.collection_id}`);
    }
  };

  const handleExportProvenance = async () => {
    setExporting(true);
    try {
      const response = await fetch(`/api/v1/adapters/${adapterId}/training-export`);
      if (!response.ok) {
        throw new Error('Failed to export training provenance');
      }
      const data = await response.json();

      // Create download as JSON file
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `training-provenance-${adapterId}.json`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);

      toast.success('Training provenance exported successfully');
    } catch (error) {
      console.error('Failed to export training provenance:', error);
      toast.error('Failed to export training provenance');
    } finally {
      setExporting(false);
    }
  };

  if (loading) {
    return (
      <Card>
        <CardContent className="py-6">
          <div className="animate-pulse space-y-3">
            <div className="h-4 bg-slate-200 rounded w-3/4" />
            <div className="h-4 bg-slate-200 rounded w-1/2" />
            <div className="h-4 bg-slate-200 rounded w-2/3" />
          </div>
        </CardContent>
      </Card>
    );
  }

  if (!snapshot) {
    return (
      <Card>
        <CardContent className="py-6 text-center text-muted-foreground">
          <Hash className="h-12 w-12 mx-auto mb-3 opacity-50" />
          <p>No training provenance available for this adapter.</p>
          <p className="text-sm mt-1">
            This adapter may not have been trained through the training pipeline.
          </p>
        </CardContent>
      </Card>
    );
  }

  return (
    <>
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle className="flex items-center gap-2">
                <Hash className="h-5 w-5" />
                Training Provenance
              </CardTitle>
              <CardDescription>
                Documents and configuration used to train this adapter
              </CardDescription>
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={handleExportProvenance}
              disabled={exporting}
            >
              <Download className="h-4 w-4 mr-2" />
              {exporting ? 'Exporting...' : 'Export'}
            </Button>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Collection info */}
          {snapshot.collection_name && (
            <div className="flex items-center gap-2">
              <Folder className="h-4 w-4 text-slate-500" />
              <span className="font-medium">Collection:</span>
              {snapshot.collection_id ? (
                <button
                  onClick={handleCollectionClick}
                  className="text-blue-600 hover:text-blue-800 hover:underline cursor-pointer focus:outline-hidden focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 rounded"
                >
                  {snapshot.collection_name}
                </button>
              ) : (
                <span>{snapshot.collection_name}</span>
              )}
            </div>
          )}

          {/* Training date */}
          <div className="flex items-center gap-2">
            <Calendar className="h-4 w-4 text-slate-500" />
            <span className="font-medium">Trained:</span>
            <span>{new Date(snapshot.created_at).toLocaleDateString()}</span>
          </div>

          {/* Training job ID */}
          <div className="flex items-center gap-2">
            <Hash className="h-4 w-4 text-slate-500" />
            <span className="font-medium">Job ID:</span>
            <span className="text-sm font-mono text-muted-foreground">
              {snapshot.training_job_id}
            </span>
          </div>

          {/* Document list */}
          <div>
            <h4 className="font-medium mb-2 flex items-center gap-2">
              <FileText className="h-4 w-4" />
              Training Documents ({snapshot.documents.length})
            </h4>
            <div className="space-y-2 max-h-64 overflow-auto">
              {snapshot.documents.map(doc => (
                <div
                  key={doc.doc_id}
                  className="flex justify-between items-center p-3 bg-slate-50 rounded text-sm hover:bg-slate-100 transition-colors"
                >
                  <div className="flex-1 min-w-0">
                    <div className="font-medium truncate">{doc.doc_name}</div>
                    <div className="text-xs text-slate-400 font-mono mt-1">
                      Hash: {doc.doc_hash.substring(0, 16)}...
                    </div>
                  </div>
                  <div className="flex items-center gap-2 ml-4">
                    <Badge variant="secondary">{doc.page_count} pages</Badge>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleViewDocument(doc)}
                    >
                      <ExternalLink className="h-4 w-4" />
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Manifest hash (for power users) */}
          <div className="text-xs text-slate-400 pt-2 border-t">
            <div className="flex items-center gap-2">
              <span className="font-medium">Chunk manifest hash:</span>
              <span className="font-mono">{snapshot.chunk_manifest_hash.substring(0, 16)}...</span>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* PDF Viewer Modal */}
      {selectedDocument && (
        <PDFViewer
          documentId={selectedDocument.doc_id}
          documentName={selectedDocument.doc_name}
          isOpen={isPdfViewerOpen}
          onClose={() => {
            setIsPdfViewerOpen(false);
            setSelectedDocument(null);
          }}
        />
      )}
    </>
  );
}
