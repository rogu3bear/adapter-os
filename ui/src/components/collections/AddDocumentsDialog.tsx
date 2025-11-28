import React, { useState, useEffect } from 'react';
import { Plus, FileText } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger, DialogFooter } from '@/components/ui/dialog';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { TERMS } from '@/constants/terminology';

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

  const fetchAvailableDocuments = React.useCallback(async () => {
    setLoading(true);
    try {
      // Fetch documents not already in this collection
      const response = await fetch(`/api/v1/collections/${collectionId}/available-documents`);
      if (response.ok) {
        const data = await response.json();
        setDocuments(data);
      }
    } catch (error) {
      console.error('Failed to fetch available documents:', error);
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
      const response = await fetch(`/api/v1/collections/${collectionId}/documents`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          document_ids: Array.from(selectedIds)
        }),
      });

      if (response.ok) {
        setOpen(false);
        setSelectedIds(new Set());
        onDocumentsAdded?.();
      }
    } catch (error) {
      console.error('Failed to add documents:', error);
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
            disabled={selectedIds.size === 0 || loading}
          >
            Add {selectedIds.size > 0 && `(${selectedIds.size})`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
