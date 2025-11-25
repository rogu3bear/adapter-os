import React from 'react';
import { DocumentUploader } from './DocumentUploader';
import { DocumentTable } from './DocumentTable';
import { useToast } from '@/hooks/use-toast';
import { useDocuments, useDocumentsApi } from '@/hooks/useDocumentsApi';

export function DocumentLibrary() {
  const { toast } = useToast();
  const { data: documents, isLoading, error, refetch } = useDocuments();
  const { deleteDocument, isDeleting } = useDocumentsApi();

  // Show error toast if fetching documents fails
  React.useEffect(() => {
    if (error) {
      toast({
        title: 'Error',
        description: 'Failed to load documents',
        variant: 'destructive',
      });
    }
  }, [error, toast]);

  const handleDelete = async (id: string) => {
    try {
      await deleteDocument(id);
      toast({
        title: 'Document Deleted',
        description: 'Document removed successfully',
      });
    } catch (error) {
      toast({
        title: 'Error',
        description: 'Failed to delete document',
        variant: 'destructive',
      });
    }
  };

  return (
    <div className="container mx-auto py-6 space-y-6">
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-2xl font-bold">Documents</h1>
          <p className="text-muted-foreground">
            Upload and manage your documents for chat and training
          </p>
        </div>
      </div>

      <DocumentUploader />

      <DocumentTable
        documents={documents ?? []}
        loading={isLoading}
        onDelete={handleDelete}
        onRefresh={refetch}
        isDeleting={isDeleting}
      />
    </div>
  );
}

export default DocumentLibrary;
