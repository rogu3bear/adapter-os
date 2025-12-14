import React, { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { FileText, Download, Trash2, RefreshCw, Eye, MessageSquare } from 'lucide-react';
import { useDocumentsApi } from '@/hooks/documents';
import type { Document } from '@/api/document-types';
import { logger, toError } from '@/utils/logger';
import { formatBytes, formatTimestamp } from '@/utils/format';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/components/ui/alert-dialog';

interface Props {
  documents: Document[];
  loading: boolean;
  onDelete: (id: string) => void;
  onRefresh: () => void;
  isDeleting?: boolean;
}

export function DocumentTable({ documents, loading, onDelete, onRefresh, isDeleting }: Props) {
  const navigate = useNavigate();
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const { downloadDocument } = useDocumentsApi();


  const getStatusBadge = (status: Document['status']) => {
    const variants = {
      processing: { variant: 'secondary' as const, label: 'Processing' },
      indexed: { variant: 'default' as const, label: 'Indexed' },
      failed: { variant: 'destructive' as const, label: 'Failed' },
      archived: { variant: 'outline' as const, label: 'Archived' },
    };

    const config = variants[status] || variants.indexed;
    return <Badge variant={config.variant}>{config.label}</Badge>;
  };

  const handleDelete = async (id: string) => {
    setDeletingId(id);
    await onDelete(id);
    setDeletingId(null);
  };

  const handleViewChunks = (_documentId: string) => {
    // TODO: Navigate to chunks view or open modal
    // Chunks view feature will be implemented in future iteration
  };

  const handleDownload = async (document: Document) => {
    try {
      const blob = await downloadDocument(document.document_id);
      const url = window.URL.createObjectURL(blob);
      const a = window.document.createElement('a');
      a.href = url;
      a.download = document.name;
      a.click();
      window.URL.revokeObjectURL(url);
    } catch (error) {
      logger.error('Document download failed', {
        component: 'DocumentTable',
        operation: 'downloadDocument',
        errorType: 'document_download_failure',
        details: 'Failed to download document from server',
        documentId: document.document_id,
        documentName: document.name,
        documentSize: document.size_bytes
      }, toError(error));
    }
  };

  if (loading) {
    return (
      <Card>
        <CardContent className="py-12 text-center">
          <RefreshCw className="h-8 w-8 animate-spin mx-auto text-muted-foreground" />
          <p className="mt-4 text-muted-foreground">Loading documents...</p>
        </CardContent>
      </Card>
    );
  }

  if (documents.length === 0) {
    return (
      <Card>
        <CardContent className="py-12 text-center">
          <FileText className="h-12 w-12 mx-auto text-muted-foreground mb-4" />
          <p className="text-lg font-medium mb-2">No documents yet</p>
          <p className="text-muted-foreground">
            Upload your first document to get started
          </p>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card data-cy="documents-card">
      <CardHeader className="flex flex-row items-center justify-end">
        <Button variant="outline" size="sm" onClick={onRefresh}>
          <RefreshCw className="h-4 w-4 mr-2" />
          Refresh
        </Button>
      </CardHeader>
      <CardContent>
        <Table data-cy="documents-table">
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead>Pages</TableHead>
              <TableHead>Size</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>Created</TableHead>
              <TableHead className="text-right">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {documents.map((doc) => (
              <TableRow
                key={doc.document_id}
                data-cy="document-row"
                data-doc-id={doc.document_id}
                data-doc-name={doc.name}
              >
                <TableCell className="font-medium">
                  <div className="flex items-center space-x-2">
                    <FileText className="h-4 w-4 text-muted-foreground" />
                    <span>{doc.name}</span>
                  </div>
                </TableCell>
                <TableCell>
                  {doc.chunk_count !== null ? doc.chunk_count : '-'}
                </TableCell>
                <TableCell>{formatBytes(doc.size_bytes)}</TableCell>
                <TableCell>{getStatusBadge(doc.status)}</TableCell>
                <TableCell className="text-muted-foreground">
                  {formatTimestamp(doc.created_at, 'long')}
                </TableCell>
                <TableCell className="text-right">
                  <div className="flex justify-end space-x-2">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleViewChunks(doc.document_id)}
                      title="View chunks"
                    >
                      <Eye className="h-4 w-4" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => navigate(`/documents/${doc.document_id}/chat`)}
                      disabled={doc.status !== 'indexed'}
                      title={doc.status !== 'indexed' ? 'Document must be indexed first' : 'Chat with document'}
                    >
                      <MessageSquare className="h-4 w-4" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleDownload(doc)}
                      title="Download"
                    >
                      <Download className="h-4 w-4" />
                    </Button>
                    <AlertDialog>
                      <AlertDialogTrigger asChild>
                        <Button
                          variant="ghost"
                          size="sm"
                          disabled={deletingId === doc.document_id || isDeleting}
                          title="Delete"
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </AlertDialogTrigger>
                      <AlertDialogContent>
                        <AlertDialogHeader>
                          <AlertDialogTitle>Delete Document</AlertDialogTitle>
                          <AlertDialogDescription>
                            Are you sure you want to delete "{doc.name}"? This
                            action cannot be undone.
                          </AlertDialogDescription>
                        </AlertDialogHeader>
                        <AlertDialogFooter>
                          <AlertDialogCancel disabled={isDeleting}>
                            Cancel
                          </AlertDialogCancel>
                          <AlertDialogAction
                            onClick={() => handleDelete(doc.document_id)}
                            disabled={isDeleting}
                          >
                            {isDeleting ? 'Deleting...' : 'Delete'}
                          </AlertDialogAction>
                        </AlertDialogFooter>
                      </AlertDialogContent>
                    </AlertDialog>
                  </div>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}
