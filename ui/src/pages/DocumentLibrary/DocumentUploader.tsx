import React, { useCallback, useState } from 'react';
import { useDropzone } from 'react-dropzone';
import { Upload, FileText, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { Card, CardContent } from '@/components/ui/card';
import { useDocumentsApi } from '@/hooks/documents';
import { useToast } from '@/hooks/use-toast';
import { logger, toError } from '@/utils/logger';

export function DocumentUploader() {
  const { toast } = useToast();
  const { uploadDocument, isUploading } = useDocumentsApi();
  const [progress, setProgress] = useState(0);
  const [pendingFiles, setPendingFiles] = useState<File[]>([]);

  const onDrop = useCallback((acceptedFiles: File[]) => {
    setPendingFiles(prev => [...prev, ...acceptedFiles]);
  }, []);

  const { getRootProps, getInputProps, isDragActive } = useDropzone({
    onDrop,
    accept: {
      'application/pdf': ['.pdf'],
      'text/markdown': ['.md'],
      'text/plain': ['.txt'],
    },
    maxSize: 50 * 1024 * 1024, // 50MB
    disabled: isUploading,
  });

  const removeFile = (index: number) => {
    setPendingFiles(prev => prev.filter((_, i) => i !== index));
  };

  const uploadFiles = async () => {
    for (let i = 0; i < pendingFiles.length; i++) {
      const file = pendingFiles[i];

      try {
        await uploadDocument({ file, name: file.name });
        toast({
          title: 'Upload Complete',
          description: `${file.name} uploaded successfully`,
        });
      } catch (error) {
        logger.error('Document upload failed', {
          component: 'DocumentUploader',
          operation: 'uploadFiles',
          errorType: 'document_upload_failure',
          details: 'Failed to upload document to server',
          fileName: file.name,
          fileSize: file.size,
          fileType: file.type,
          uploadIndex: i,
          totalFiles: pendingFiles.length
        }, toError(error));
        toast({
          title: 'Upload Failed',
          description: `Failed to upload ${file.name}`,
          variant: 'destructive',
        });
      }
      setProgress(((i + 1) / pendingFiles.length) * 100);
    }
    setPendingFiles([]);
    setProgress(0);
  };

  const formatFileSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  return (
    <Card>
      <CardContent className="pt-6">
        <div
          {...getRootProps()}
          data-cy="document-dropzone"
          className={`border-2 border-dashed rounded-lg p-8 text-center cursor-pointer transition-colors ${
            isDragActive
              ? 'border-primary bg-primary/5'
              : 'border-gray-300 hover:border-primary/50'
          } ${isUploading ? 'opacity-50 cursor-not-allowed' : ''}`}
        >
          <input
            {...getInputProps()}
            name="file"
            data-cy="document-file-input"
          />
          <Upload className="mx-auto h-12 w-12 text-gray-400 mb-4" />
          {isDragActive ? (
            <p className="text-lg font-medium">Drop files here...</p>
          ) : (
            <>
              <p className="text-lg font-medium mb-2">
                Drag & drop documents here, or click to select
              </p>
              <p className="text-sm text-muted-foreground">
                Supports PDF, Markdown, and Text files (max 50MB)
              </p>
            </>
          )}
        </div>

        {pendingFiles.length > 0 && (
          <div className="mt-6 space-y-4">
            <div className="space-y-2">
              {pendingFiles.map((file, index) => (
                <div
                  key={index}
                  className="flex items-center justify-between p-3 bg-gray-50 rounded-lg"
                >
                  <div className="flex items-center space-x-3">
                    <FileText className="h-5 w-5 text-blue-500" />
                    <div>
                      <p className="text-sm font-medium">{file.name}</p>
                      <p className="text-xs text-muted-foreground">
                        {formatFileSize(file.size)}
                      </p>
                    </div>
                  </div>
                  {!isUploading && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => removeFile(index)}
                    >
                      <X className="h-4 w-4" />
                    </Button>
                  )}
                </div>
              ))}
            </div>

            {isUploading && (
              <div className="space-y-2">
                <div className="flex justify-between text-sm">
                  <span>Uploading...</span>
                  <span>{Math.round(progress)}%</span>
                </div>
                <Progress value={progress} />
              </div>
            )}

            <div className="flex justify-end space-x-2">
              {!isUploading && (
                <>
                  <Button
                    variant="outline"
                    onClick={() => setPendingFiles([])}
                  >
                    Clear All
                  </Button>
                  <Button data-cy="document-upload-button" onClick={uploadFiles}>
                    Upload {pendingFiles.length} File
                    {pendingFiles.length !== 1 ? 's' : ''}
                  </Button>
                </>
              )}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
