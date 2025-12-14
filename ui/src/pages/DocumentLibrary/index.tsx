import React from 'react';
import { DocumentUploader } from './DocumentUploader';
import { DocumentTable } from './DocumentTable';
import PageWrapper from '@/layout/PageWrapper';
import { useToast } from '@/hooks/use-toast';
import { useDocuments, useDocumentsApi } from '@/hooks/documents';

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
    <PageWrapper
      pageKey="documents"
      title="Documents"
      description="Upload and manage your documents for chat and training"
      maxWidth="xl"
    >
      <div className="space-y-6">
        <DocumentUploader />
        <DocumentTable
          documents={documents ?? []}
          loading={isLoading}
          onDelete={handleDelete}
          onRefresh={refetch}
          isDeleting={isDeleting}
        />
      </div>
    </PageWrapper>
  );
}

export default DocumentLibrary;
