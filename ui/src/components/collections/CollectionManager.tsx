import React, { useState } from 'react';
import { Plus, Folder, Trash2, FileText } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { TERMS } from '@/constants/terminology';
import { useCollections, useCollectionsApi } from '@/hooks/api/useCollectionsApi';
import type { Collection } from '@/api/document-types';
import { logger, toError } from '@/utils/logger';

interface Props {
  onSelectCollection?: (collection: Collection) => void;
  selectedCollectionId?: string;
}

export function CollectionManager({ onSelectCollection, selectedCollectionId }: Props) {
  const [createOpen, setCreateOpen] = useState(false);
  const [newName, setNewName] = useState('');
  const [newDescription, setNewDescription] = useState('');

  // Use React Query hooks for data fetching and mutations
  const { data: collections, isLoading, error } = useCollections();
  const {
    createCollection: createCollectionMutation,
    deleteCollection: deleteCollectionMutation,
    isCreating,
    isDeleting,
  } = useCollectionsApi();

  const handleCreateCollection = async () => {
    if (!newName.trim()) return;

    try {
      await createCollectionMutation({
        name: newName.trim(),
        description: newDescription.trim() || undefined,
      });
      setCreateOpen(false);
      setNewName('');
      setNewDescription('');
    } catch (error) {
      logger.error('Collection creation failed', {
        component: 'CollectionManager',
        operation: 'handleCreateCollection',
        errorType: 'collection_create_failure',
        details: 'Failed to create new document collection',
        collectionName: newName,
        collectionDescription: newDescription
      }, toError(error));
    }
  };

  const handleDeleteCollection = async (id: string | undefined) => {
    if (!id || !confirm('Are you sure you want to delete this collection?')) return;

    try {
      await deleteCollectionMutation(id);
    } catch (error) {
      logger.error('Collection deletion failed', {
        component: 'CollectionManager',
        operation: 'handleDeleteCollection',
        errorType: 'collection_delete_failure',
        details: 'Failed to delete document collection',
        collectionId: id
      }, toError(error));
    }
  };

  if (isLoading) {
    return <div className="text-sm text-muted-foreground">Loading collections...</div>;
  }

  if (error) {
    return <div className="text-sm text-red-500">Failed to load collections</div>;
  }

  return (
    <div className="space-y-4">
      <div className="flex justify-between items-center">
        <h3 className="text-lg font-medium">Collections</h3>
        <Dialog open={createOpen} onOpenChange={setCreateOpen}>
          <DialogTrigger asChild>
            <Button size="sm">
              <Plus className="h-4 w-4 mr-2" />
              New Collection
            </Button>
          </DialogTrigger>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>Create Collection</DialogTitle>
            </DialogHeader>
            <div className="space-y-4">
              <div className="space-y-2">
                <label htmlFor="collection-name" className="text-sm font-medium">
                  {TERMS.datasetName}
                </label>
                <Input
                  id="collection-name"
                  placeholder="Enter collection name"
                  value={newName}
                  onChange={e => setNewName(e.target.value)}
                  autoFocus
                />
              </div>
              <div className="space-y-2">
                <label htmlFor="collection-description" className="text-sm font-medium">
                  {TERMS.datasetDescription}
                </label>
                <Input
                  id="collection-description"
                  placeholder="Optional description"
                  value={newDescription}
                  onChange={e => setNewDescription(e.target.value)}
                />
              </div>
              <div className="flex justify-end gap-2">
                <Button variant="outline" onClick={() => setCreateOpen(false)}>
                  {TERMS.cancel}
                </Button>
                <Button onClick={handleCreateCollection} disabled={!newName.trim() || isCreating}>
                  {isCreating ? 'Creating...' : TERMS.create}
                </Button>
              </div>
            </div>
          </DialogContent>
        </Dialog>
      </div>

      {/* Collection list */}
      <div className="space-y-2">
        {!collections || collections.length === 0 ? (
          <div className="text-center py-8 text-sm text-muted-foreground">
            No collections yet. Create one to get started.
          </div>
        ) : (
          collections.map(collection => (
            <div
              key={collection.id}
              className={`p-3 rounded-lg border cursor-pointer hover:bg-slate-50 transition-colors ${
                selectedCollectionId === collection.id ? 'border-blue-500 bg-blue-50' : ''
              }`}
              onClick={() => onSelectCollection?.(collection)}
            >
              <div className="flex justify-between items-center">
                <div className="flex items-center gap-2 flex-1">
                  <Folder className="h-4 w-4 text-muted-foreground" />
                  <div className="flex-1">
                    <div className="font-medium">{collection.name}</div>
                    {collection.description && (
                      <div className="text-xs text-muted-foreground mt-0.5">
                        {collection.description}
                      </div>
                    )}
                  </div>
                </div>
                <div className="flex items-center gap-3">
                  <div className="flex items-center gap-1.5 text-sm text-muted-foreground">
                    <FileText className="h-3 w-3" />
                    <span>{collection.document_count} docs</span>
                  </div>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDeleteCollection(collection.id);
                    }}
                    disabled={isDeleting}
                    className="opacity-0 group-hover:opacity-100 transition-opacity"
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </Button>
                </div>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
