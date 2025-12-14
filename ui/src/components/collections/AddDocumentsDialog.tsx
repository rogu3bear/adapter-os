import React, { useState, useEffect } from 'react';
import { Plus, FileText, AlertTriangle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger, DialogFooter } from '@/components/ui/dialog';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { TERMS } from '@/constants/terminology';
import { Alert, AlertDescription } from '@/components/ui/alert';
import useCollectionsApi from '@/hooks/api/useCollectionsApi';
import apiClient from '@/api/client';
import { logger, toError } from '@/utils/logger';

interface Document {
  id: string;
  name: string;
  type: string;
  size: number;
  created_at: string;
}

interface Props {
  collectionId: string;
  onDocumentsAdded?: () => void;
}

export function AddDocumentsDialog({ collectionId, onDocumentsAdded }: Props) {
  const [open, setOpen] = useState(false);
  const [documents, setDocuments] = useState<Document[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [featureUnavailable, setFeatureUnavailable] = useState<string | null>(null);
  const { invalidateCollections } = useCollectionsApi();

  const fetchAvailableDocuments = React.useCallback(async () => {
    setLoading(true);
    try {
      const data = await apiClient.listAvailableDocuments(collectionId);
      // Ensure data is properly mapped to Document type
      const mappedDocuments: Document[] = Array.isArray(data)
        ? data.map((doc: any) => ({
            id: doc.id ?? doc.document_id ?? '',
            name: doc.name ?? '',
            type: doc.type ?? doc.mime_type ?? '',
            size: doc.size ?? 0,
            created_at: doc.created_at ?? new Date().toISOString(),
          }))
        : [];
      setDocuments(mappedDocuments);
      setFeatureUnavailable(null);
    } catch (error) {
      // Surface feature gating separately; all other failures are logged
      const err = toError(error);
      if ((error as { code?: string; status?: number }).status === 404) {
        setFeatureUnavailable('Collection document endpoints are not available on this backend (v0.9).');
        setDocuments([]);
      } else {
        logger.error('Failed to fetch available documents', {
          component: 'AddDocumentsDialog',
          operation: 'listAvailableDocuments',
          collectionId,
        }, err);
        setDocuments([]);
      }
    } finally {
      setLoading(false);
    }
  }, [collectionId]);

  useEffect(() => {
    if (open) {
      fetchAvailableDocuments();
    }
  }, [open, fetchAvailableDocuments]);

  const toggleDocument = (docId: string) => {
    setSelectedIds(prev => {
      const next = new Set(prev);
      if (next.has(docId)) {
        next.delete(docId);
      } else {
        next.add(docId);
      }
      return next;
    });
  };

  const addDocuments = async () => {
    if (selectedIds.size === 0) return;

    setLoading(true);
    try {
      await apiClient.addDocumentsToCollection(collectionId, Array.from(selectedIds));
      setOpen(false);
      setSelectedIds(new Set());
      await invalidateCollections();
      onDocumentsAdded?.();
    } catch (error) {
      const err = toError(error);
      if ((error as { code?: string; status?: number }).status === 404) {
        setFeatureUnavailable('Collection document endpoints are not available on this backend (v0.9).');
      } else {
        logger.error('Failed to add documents to collection', {
          component: 'AddDocumentsDialog',
          operation: 'addDocumentsToCollection',
          collectionId,
          selectedCount: selectedIds.size,
        }, err);
      }
    } finally {
      setLoading(false);
    }
  };

  const filteredDocuments = documents.filter(doc =>
    doc.name.toLowerCase().includes(searchQuery.toLowerCase())
  );

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button size="sm" variant="outline">
          <Plus className="h-4 w-4 mr-2" />
          Add Documents
        </Button>
      </DialogTrigger>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>Add Documents to Collection</DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          {featureUnavailable && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>{featureUnavailable}</AlertDescription>
            </Alert>
          )}

          {/* Search filter */}
          <Input
            placeholder="Search documents..."
            value={searchQuery}
            onChange={e => setSearchQuery(e.target.value)}
          />

          {/* Document list */}
          <div className="max-h-96 overflow-y-auto space-y-2 border rounded-lg p-2">
            {loading ? (
              <div className="text-center py-8 text-sm text-muted-foreground">
                Loading available documents...
              </div>
            ) : filteredDocuments.length === 0 ? (
              <div className="text-center py-8 text-sm text-muted-foreground">
                {searchQuery ? 'No matching documents found' : 'No documents available to add'}
              </div>
            ) : (
              filteredDocuments.map(doc => (
                <div
                  key={doc.id}
                  className="flex items-center gap-3 p-2 rounded hover:bg-slate-50 cursor-pointer"
                  onClick={() => toggleDocument(doc.id)}
                >
                  <Checkbox
                    checked={selectedIds.has(doc.id)}
                    onCheckedChange={() => toggleDocument(doc.id)}
                  />
                  <FileText className="h-4 w-4 text-muted-foreground flex-shrink-0" />
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium truncate">{doc.name}</div>
                    <div className="text-xs text-muted-foreground">
                      {doc.type} • {(doc.size / 1024).toFixed(1)} KB
                    </div>
                  </div>
                </div>
              ))
            )}
          </div>

          {/* Selection summary */}
          {selectedIds.size > 0 && (
            <div className="text-sm text-muted-foreground">
              {selectedIds.size} document{selectedIds.size !== 1 ? 's' : ''} selected
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)}>
            {TERMS.cancel}
          </Button>
          <Button
            onClick={addDocuments}
            disabled={selectedIds.size === 0 || loading || !!featureUnavailable}
          >
            Add {selectedIds.size > 0 && `(${selectedIds.size})`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
