// DatasetBuilder - Multi-tab document upload and dataset configuration
// Citation: CLAUDE.md - Follow density-aware patterns, use structured error handling

import React, { useState, useRef, useCallback, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '../ui/card';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '../ui/tabs';
import { Progress } from '../ui/progress';
import { Badge } from '../ui/badge';
import { Alert, AlertDescription } from '../ui/alert';
import { Textarea } from '../ui/textarea';
import {
  Upload,
  File,
  FileText,
  X,
  CheckCircle,
  AlertCircle,
  Folder,
  FolderOpen,
  Settings,
  Eye,
  Check,
  Loader2,
  RefreshCw,
  Download,
  Plus,
  Trash2
} from 'lucide-react';
import { logger, toError } from '@/utils/logger';
import { ErrorRecovery } from '../ui/error-recovery';
import { useInformationDensity } from '@/hooks/useInformationDensity';
import { DatasetConfigSchema, formatValidationError } from '@/schemas';

// File upload state
interface UploadedFile {
  id: string;
  file: File;
  status: 'pending' | 'uploading' | 'complete' | 'error';
  progress: number;
  error?: string;
  preview?: string;
}

// Dataset configuration
interface DatasetConfig {
  name: string;
  description?: string;
  strategy: 'identity' | 'question_answer' | 'masked_lm';
  maxSequenceLength: number;
  validationSplit: number;
  tokenizer?: string;
}

// File validation rules
const FILE_VALIDATION = {
  maxSize: 100 * 1024 * 1024, // 100MB per file
  allowedExtensions: ['.pdf', '.txt', '.json', '.jsonl', '.csv', '.md', '.py', '.js', '.ts', '.tsx', '.jsx', '.rs', '.go', '.java'],
  allowedTypes: ['application/pdf', 'text/plain', 'application/json', 'text/csv', 'text/markdown']
};

export interface DatasetBuilderProps {
  onDatasetCreated?: (datasetId: string, config: DatasetConfig) => void;
  onCancel?: () => void;
  initialConfig?: Partial<DatasetConfig>;
}

export function DatasetBuilder({ onDatasetCreated, onCancel, initialConfig }: DatasetBuilderProps) {
  const [activeTab, setActiveTab] = useState<'upload' | 'configure' | 'preview' | 'validate'>('upload');
  const [files, setFiles] = useState<UploadedFile[]>([]);
  const [isDragging, setIsDragging] = useState(false);
  const [uploadError, setUploadError] = useState<Error | null>(null);
  const [isProcessing, setIsProcessing] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const directoryInputRef = useRef<HTMLInputElement>(null);
  const { density } = useInformationDensity();

  // Dataset configuration state
  const [config, setConfig] = useState<DatasetConfig>({
    name: '',
    description: '',
    strategy: 'identity',
    maxSequenceLength: 2048,
    validationSplit: 0.1,
    tokenizer: undefined,
    ...initialConfig
  });

  // Draft persistence - save to localStorage
  useEffect(() => {
    const draftKey = 'dataset-builder-draft';
    const savedDraft = localStorage.getItem(draftKey);
    if (savedDraft && !initialConfig) {
      try {
        const parsed = JSON.parse(savedDraft);
        setConfig(parsed.config || config);
        logger.info('Restored draft from localStorage', { component: 'DatasetBuilder' });
      } catch (error) {
        logger.error('Failed to restore draft', { component: 'DatasetBuilder' }, toError(error));
      }
    }
  }, []);

  useEffect(() => {
    const draftKey = 'dataset-builder-draft';
    if (config.name || config.description || files.length > 0) {
      localStorage.setItem(draftKey, JSON.stringify({ config, fileCount: files.length }));
    }
  }, [config, files]);

  // Validate file
  const validateFile = (file: File): string | null => {
    // Check size
    if (file.size > FILE_VALIDATION.maxSize) {
      return `File size exceeds ${FILE_VALIDATION.maxSize / (1024 * 1024)}MB limit`;
    }

    // Check extension
    const extension = '.' + file.name.split('.').pop()?.toLowerCase();
    if (!FILE_VALIDATION.allowedExtensions.includes(extension)) {
      return `File type ${extension} not supported. Supported types: ${FILE_VALIDATION.allowedExtensions.join(', ')}`;
    }

    return null;
  };

  // Handle file selection
  const handleFileSelect = useCallback((selectedFiles: FileList | null) => {
    if (!selectedFiles) return;

    const newFiles: UploadedFile[] = [];
    const errors: string[] = [];

    Array.from(selectedFiles).forEach((file) => {
      const error = validateFile(file);
      if (error) {
        errors.push(`${file.name}: ${error}`);
        return;
      }

      // Check for duplicates
      const isDuplicate = files.some(f => f.file.name === file.name && f.file.size === file.size);
      if (isDuplicate) {
        errors.push(`${file.name}: Already added`);
        return;
      }

      newFiles.push({
        id: `${Date.now()}-${Math.random()}`,
        file,
        status: 'pending',
        progress: 0
      });
    });

    if (errors.length > 0) {
      setUploadError(new Error(errors.join('\n')));
    }

    if (newFiles.length > 0) {
      setFiles(prev => [...prev, ...newFiles]);
      logger.info('Files selected', { component: 'DatasetBuilder', count: newFiles.length });
    }
  }, [files]);

  // Handle drag and drop
  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(true);
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(false);
  }, []);

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(false);

    const droppedFiles = e.dataTransfer.files;
    handleFileSelect(droppedFiles);
  }, [handleFileSelect]);

  // Handle directory upload
  const handleDirectorySelect = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const selectedFiles = e.target.files;
    handleFileSelect(selectedFiles);
  }, [handleFileSelect]);

  // Remove file
  const removeFile = useCallback((fileId: string) => {
    setFiles(prev => prev.filter(f => f.id !== fileId));
  }, []);

  // Retry failed upload
  const retryUpload = useCallback((fileId: string) => {
    setFiles(prev => prev.map(f =>
      f.id === fileId ? { ...f, status: 'pending', progress: 0, error: undefined } : f
    ));
  }, []);

  // Upload files to server (simulated for now)
  const uploadFiles = useCallback(async () => {
    setIsProcessing(true);
    setUploadError(null);

    try {
      // Upload each file
      for (const fileData of files) {
        if (fileData.status === 'complete') continue;

        // Update status to uploading
        setFiles(prev => prev.map(f =>
          f.id === fileData.id ? { ...f, status: 'uploading' as const } : f
        ));

        // Simulate upload with progress (in real implementation, use FormData and track progress)
        const simulateUpload = async () => {
          for (let progress = 0; progress <= 100; progress += 10) {
            await new Promise(resolve => setTimeout(resolve, 100));
            setFiles(prev => prev.map(f =>
              f.id === fileData.id ? { ...f, progress } : f
            ));
          }
        };

        await simulateUpload();

        // Mark as complete
        setFiles(prev => prev.map(f =>
          f.id === fileData.id ? { ...f, status: 'complete' as const, progress: 100 } : f
        ));

        logger.info('File uploaded', { component: 'DatasetBuilder', fileName: fileData.file.name });
      }

      // Move to configure tab after successful upload
      setActiveTab('configure');
    } catch (error) {
      const err = toError(error);
      setUploadError(err);
      logger.error('Upload failed', { component: 'DatasetBuilder' }, err);
    } finally {
      setIsProcessing(false);
    }
  }, [files]);

  // Generate file preview
  const generatePreview = useCallback(async (file: File): Promise<string> => {
    return new Promise((resolve, reject) => {
      if (file.type.startsWith('text/') || file.name.endsWith('.md') || file.name.endsWith('.json')) {
        const reader = new FileReader();
        reader.onload = (e) => {
          const content = e.target?.result as string;
          resolve(content.slice(0, 500)); // First 500 chars
        };
        reader.onerror = reject;
        reader.readAsText(file);
      } else if (file.type === 'application/pdf') {
        resolve('[PDF Document - Preview not available]');
      } else {
        resolve('[Binary file - Preview not available]');
      }
    });
  }, []);

  // File statistics
  const fileStats = {
    total: files.length,
    pending: files.filter(f => f.status === 'pending').length,
    uploading: files.filter(f => f.status === 'uploading').length,
    complete: files.filter(f => f.status === 'complete').length,
    error: files.filter(f => f.status === 'error').length,
    totalSize: files.reduce((acc, f) => acc + f.file.size, 0)
  };

  // Format file size
  const formatSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  // Validate dataset configuration
  const validateConfig = (): string[] => {
    const errors: string[] = [];
    if (!config.name.trim()) errors.push('Dataset name is required');
    if (config.maxSequenceLength < 128) errors.push('Max sequence length must be at least 128');
    if (config.validationSplit < 0 || config.validationSplit > 0.5) errors.push('Validation split must be between 0 and 0.5');
    if (files.length === 0) errors.push('At least one file is required');
    return errors;
  };

  // Create dataset
  const createDataset = async () => {
    // Validate using Zod schema
    try {
      await DatasetConfigSchema.parseAsync(config);
    } catch (error) {
      if (error instanceof Error && error.name === 'ZodError') {
        const validationResult = formatValidationError(error as any);
        const errorMessages = validationResult.errors.map(e => e.message);
        setUploadError(new Error(errorMessages.join('\n')));
        return;
      }
    }

    // Also check files requirement (not in schema)
    if (files.length === 0) {
      setUploadError(new Error('At least one file is required'));
      return;
    }

    setIsProcessing(true);
    try {
      // In real implementation, call API to create dataset
      // const datasetId = await apiClient.createDataset({ config, files });

      const datasetId = `dataset-${Date.now()}`;
      logger.info('Dataset created', { component: 'DatasetBuilder', datasetId });

      // Clear draft
      localStorage.removeItem('dataset-builder-draft');

      onDatasetCreated?.(datasetId, config);
    } catch (error) {
      const err = toError(error);
      setUploadError(err);
      logger.error('Dataset creation failed', { component: 'DatasetBuilder' }, err);
    } finally {
      setIsProcessing(false);
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold">Dataset Builder</h2>
        <p className="text-muted-foreground">Upload documents and configure training dataset</p>
      </div>

      <Tabs value={activeTab} onValueChange={(value) => setActiveTab(value as any)}>
        <TabsList>
          <TabsTrigger value="upload">
            <Upload className="w-4 h-4" />
            Upload
            {fileStats.total > 0 && <Badge variant="secondary" className="ml-2">{fileStats.total}</Badge>}
          </TabsTrigger>
          <TabsTrigger value="configure">
            <Settings className="w-4 h-4" />
            Configure
          </TabsTrigger>
          <TabsTrigger value="preview">
            <Eye className="w-4 h-4" />
            Preview
          </TabsTrigger>
          <TabsTrigger value="validate">
            <Check className="w-4 h-4" />
            Validate
          </TabsTrigger>
        </TabsList>

        {/* Upload Tab */}
        <TabsContent value="upload">
          <div className="space-y-4">
            {/* Drop Zone */}
            <Card
              className={`border-2 border-dashed transition-colors ${
                isDragging ? 'border-blue-500 bg-blue-50 dark:bg-blue-950' : 'border-muted'
              }`}
              onDragOver={handleDragOver}
              onDragLeave={handleDragLeave}
              onDrop={handleDrop}
            >
              <CardContent className="pt-6">
                <div className="text-center py-12">
                  <div className="flex justify-center mb-4">
                    {isDragging ? (
                      <FolderOpen className="w-16 h-16 text-blue-500" />
                    ) : (
                      <Upload className="w-16 h-16 text-muted-foreground" />
                    )}
                  </div>
                  <h3 className="text-lg font-medium mb-2">
                    {isDragging ? 'Drop files here' : 'Upload Training Documents'}
                  </h3>
                  <p className="text-sm text-muted-foreground mb-4">
                    Drag and drop files or click to browse
                  </p>
                  <div className="flex gap-3 justify-center">
                    <Button
                      variant="outline"
                      onClick={() => fileInputRef.current?.click()}
                    >
                      <Plus className="w-4 h-4 mr-2" />
                      Select Files
                    </Button>
                    <Button
                      variant="outline"
                      onClick={() => directoryInputRef.current?.click()}
                    >
                      <Folder className="w-4 h-4 mr-2" />
                      Select Directory
                    </Button>
                  </div>
                  <input
                    ref={fileInputRef}
                    type="file"
                    multiple
                    accept={FILE_VALIDATION.allowedExtensions.join(',')}
                    onChange={(e) => handleFileSelect(e.target.files)}
                    className="hidden"
                  />
                  <input
                    ref={directoryInputRef}
                    type="file"
                    multiple
                    // @ts-ignore - webkitdirectory is not in types but works in modern browsers
                    webkitdirectory="true"
                    onChange={handleDirectorySelect}
                    className="hidden"
                  />
                  <p className="text-xs text-muted-foreground mt-4">
                    Supported formats: {FILE_VALIDATION.allowedExtensions.join(', ')}
                    <br />
                    Maximum file size: {FILE_VALIDATION.maxSize / (1024 * 1024)}MB
                  </p>
                </div>
              </CardContent>
            </Card>

            {/* File List */}
            {files.length > 0 && (
              <Card>
                <CardHeader>
                  <CardTitle className="flex items-center justify-between">
                    <span>Uploaded Files ({fileStats.total})</span>
                    <Badge variant="outline">{formatSize(fileStats.totalSize)}</Badge>
                  </CardTitle>
                  {fileStats.complete < fileStats.total && (
                    <CardDescription>
                      {fileStats.complete} of {fileStats.total} files uploaded
                    </CardDescription>
                  )}
                </CardHeader>
                <CardContent>
                  <div className="space-y-3 max-h-96 overflow-y-auto">
                    {files.map((fileData) => (
                      <div
                        key={fileData.id}
                        className="flex items-center gap-3 p-3 border rounded-lg"
                      >
                        <div className="flex-shrink-0">
                          {fileData.status === 'complete' ? (
                            <CheckCircle className="w-5 h-5 text-green-500" />
                          ) : fileData.status === 'error' ? (
                            <AlertCircle className="w-5 h-5 text-red-500" />
                          ) : fileData.status === 'uploading' ? (
                            <Loader2 className="w-5 h-5 text-blue-500 animate-spin" />
                          ) : (
                            <File className="w-5 h-5 text-muted-foreground" />
                          )}
                        </div>
                        <div className="flex-1 min-w-0">
                          <p className="font-medium truncate">{fileData.file.name}</p>
                          <div className="flex items-center gap-2 text-xs text-muted-foreground">
                            <span>{formatSize(fileData.file.size)}</span>
                            {fileData.status === 'uploading' && (
                              <>
                                <span>•</span>
                                <span>{fileData.progress}%</span>
                              </>
                            )}
                            {fileData.error && (
                              <>
                                <span>•</span>
                                <span className="text-red-500">{fileData.error}</span>
                              </>
                            )}
                          </div>
                          {fileData.status === 'uploading' && (
                            <Progress value={fileData.progress} className="mt-2 h-1" />
                          )}
                        </div>
                        <div className="flex gap-2">
                          {fileData.status === 'error' && (
                            <Button
                              size="sm"
                              variant="ghost"
                              onClick={() => retryUpload(fileData.id)}
                            >
                              <RefreshCw className="w-4 h-4" />
                            </Button>
                          )}
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() => removeFile(fileData.id)}
                          >
                            <Trash2 className="w-4 h-4" />
                          </Button>
                        </div>
                      </div>
                    ))}
                  </div>

                  {uploadError && (
                    <ErrorRecovery
                      title="Upload Error"
                      message={uploadError.message}
                      recoveryActions={[
                        { label: 'Retry', action: () => { setUploadError(null); uploadFiles(); } },
                        { label: 'Dismiss', action: () => setUploadError(null) }
                      ]}
                    />
                  )}

                  <div className="flex gap-3 mt-4">
                    {fileStats.pending > 0 && (
                      <Button onClick={uploadFiles} disabled={isProcessing} className="flex-1">
                        {isProcessing ? (
                          <>
                            <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                            Uploading...
                          </>
                        ) : (
                          <>
                            <Upload className="w-4 h-4 mr-2" />
                            Upload {fileStats.pending} File{fileStats.pending > 1 ? 's' : ''}
                          </>
                        )}
                      </Button>
                    )}
                    {fileStats.complete === fileStats.total && fileStats.total > 0 && (
                      <Button onClick={() => setActiveTab('configure')} className="flex-1">
                        Continue to Configuration
                      </Button>
                    )}
                  </div>
                </CardContent>
              </Card>
            )}
          </div>
        </TabsContent>

        {/* Configure Tab */}
        <TabsContent value="configure">
          <Card>
            <CardHeader>
              <CardTitle>Dataset Configuration</CardTitle>
              <CardDescription>Configure how the dataset will be processed</CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="dataset-name">Dataset Name *</Label>
                <Input
                  id="dataset-name"
                  value={config.name}
                  onChange={(e) => setConfig({ ...config, name: e.target.value })}
                  placeholder="my-code-dataset"
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="dataset-description">Description</Label>
                <Textarea
                  id="dataset-description"
                  value={config.description}
                  onChange={(e) => setConfig({ ...config, description: e.target.value })}
                  placeholder="Description of what this dataset contains..."
                  rows={3}
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="training-strategy">Training Strategy</Label>
                <select
                  id="training-strategy"
                  value={config.strategy}
                  onChange={(e) => setConfig({ ...config, strategy: e.target.value as any })}
                  className="w-full rounded-md border border-input bg-background px-3 py-2"
                >
                  <option value="identity">Identity (Unsupervised)</option>
                  <option value="question_answer">Question & Answer</option>
                  <option value="masked_lm">Masked Language Model</option>
                </select>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label htmlFor="max-seq-length">Max Sequence Length</Label>
                  <Input
                    id="max-seq-length"
                    type="number"
                    value={config.maxSequenceLength}
                    onChange={(e) => setConfig({ ...config, maxSequenceLength: parseInt(e.target.value) })}
                    min={128}
                    max={8192}
                  />
                </div>

                <div className="space-y-2">
                  <Label htmlFor="validation-split">Validation Split</Label>
                  <Input
                    id="validation-split"
                    type="number"
                    step="0.05"
                    value={config.validationSplit}
                    onChange={(e) => setConfig({ ...config, validationSplit: parseFloat(e.target.value) })}
                    min={0}
                    max={0.5}
                  />
                </div>
              </div>

              <div className="flex gap-3 pt-4">
                <Button variant="outline" onClick={() => setActiveTab('upload')}>
                  Back
                </Button>
                <Button onClick={() => setActiveTab('preview')} className="flex-1">
                  Preview Dataset
                </Button>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Preview Tab */}
        <TabsContent value="preview">
          <Card>
            <CardHeader>
              <CardTitle>Dataset Preview</CardTitle>
              <CardDescription>Preview sample data from your dataset</CardDescription>
            </CardHeader>
            <CardContent>
              <Alert>
                <AlertCircle className="w-4 h-4" />
                <AlertDescription>
                  Dataset preview shows the first few samples after processing.
                  Full dataset will be generated during training.
                </AlertDescription>
              </Alert>

              <div className="mt-4 space-y-3">
                {files.slice(0, 3).map((fileData, idx) => (
                  <div key={fileData.id} className="border rounded-lg p-4">
                    <div className="flex items-center gap-2 mb-2">
                      <FileText className="w-4 h-4" />
                      <span className="font-medium">{fileData.file.name}</span>
                      <Badge variant="outline">Sample {idx + 1}</Badge>
                    </div>
                    <pre className="text-xs bg-muted p-3 rounded overflow-auto max-h-32">
                      {fileData.preview || '[Loading preview...]'}
                    </pre>
                  </div>
                ))}
              </div>

              <div className="flex gap-3 mt-6">
                <Button variant="outline" onClick={() => setActiveTab('configure')}>
                  Back
                </Button>
                <Button onClick={() => setActiveTab('validate')} className="flex-1">
                  Validate Dataset
                </Button>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Validate Tab */}
        <TabsContent value="validate">
          <Card>
            <CardHeader>
              <CardTitle>Dataset Validation</CardTitle>
              <CardDescription>Verify dataset quality and completeness</CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-3">
                <div className="flex items-center gap-2">
                  <CheckCircle className="w-5 h-5 text-green-500" />
                  <span className="font-medium">Configuration Valid</span>
                </div>
                <div className="flex items-center gap-2">
                  <CheckCircle className="w-5 h-5 text-green-500" />
                  <span className="font-medium">{fileStats.complete} Files Uploaded</span>
                </div>
                <div className="flex items-center gap-2">
                  <CheckCircle className="w-5 h-5 text-green-500" />
                  <span className="font-medium">Total Size: {formatSize(fileStats.totalSize)}</span>
                </div>
              </div>

              {uploadError && (
                <ErrorRecovery
                  title="Validation Error"
                  message={uploadError.message}
                  recoveryActions={[
                    { label: 'Fix Configuration', action: () => { setUploadError(null); setActiveTab('configure'); } },
                    { label: 'Dismiss', action: () => setUploadError(null) }
                  ]}
                />
              )}

              <div className="flex gap-3 pt-4">
                <Button variant="outline" onClick={() => setActiveTab('preview')}>
                  Back
                </Button>
                <Button
                  onClick={createDataset}
                  disabled={isProcessing || validateConfig().length > 0}
                  className="flex-1"
                >
                  {isProcessing ? (
                    <>
                      <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                      Creating Dataset...
                    </>
                  ) : (
                    <>
                      <CheckCircle className="w-4 h-4 mr-2" />
                      Create Dataset
                    </>
                  )}
                </Button>
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      {onCancel && (
        <div className="flex justify-end">
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
        </div>
      )}
    </div>
  );
}
