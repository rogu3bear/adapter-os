import React, { useState } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import { Button } from '../ui/button';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '../ui/table';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '../ui/collapsible';
import { ChevronDown, ChevronRight, FileText, Check, X, AlertCircle } from 'lucide-react';
import { cn } from '../ui/utils';

interface DatasetFile {
  id: string;
  file_name: string;
  file_path: string;
  size_bytes: number;
  hash_b3: string;
  mime_type?: string;
  created_at: string;
}

interface FilePreviewData extends DatasetFile {
  tokens?: number;
  language?: string;
  status?: 'success' | 'warning' | 'error';
  content?: string;
  error?: string;
}

interface DatasetPreviewProps {
  files: FilePreviewData[];
  maxPreview?: number;
  onFileSelect?: (file: FilePreviewData) => void;
  showContent?: boolean;
  loading?: boolean;
}

const formatBytes = (bytes: number): string => {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(2)} ${sizes[i]}`;
};

const getLanguageBadgeColor = (language?: string): string => {
  if (!language) return 'default';
  const colorMap: Record<string, string> = {
    typescript: 'bg-blue-500/10 text-blue-500 border-blue-500/20',
    javascript: 'bg-yellow-500/10 text-yellow-500 border-yellow-500/20',
    python: 'bg-green-500/10 text-green-500 border-green-500/20',
    rust: 'bg-orange-500/10 text-orange-500 border-orange-500/20',
    go: 'bg-cyan-500/10 text-cyan-500 border-cyan-500/20',
    java: 'bg-red-500/10 text-red-500 border-red-500/20',
    cpp: 'bg-purple-500/10 text-purple-500 border-purple-500/20',
    c: 'bg-purple-500/10 text-purple-500 border-purple-500/20',
    default: 'bg-muted text-muted-foreground',
  };
  return colorMap[language.toLowerCase()] || colorMap.default;
};

const getStatusIcon = (status?: 'success' | 'warning' | 'error') => {
  switch (status) {
    case 'success':
      return <Check className="h-4 w-4 text-green-500" />;
    case 'warning':
      return <AlertCircle className="h-4 w-4 text-yellow-500" />;
    case 'error':
      return <X className="h-4 w-4 text-red-500" />;
    default:
      return <FileText className="h-4 w-4 text-muted-foreground" />;
  }
};

const SyntaxHighlightedCode: React.FC<{ code: string; language?: string }> = ({
  code,
  language,
}) => {
  // Simple syntax highlighting for code preview
  // In production, you might use Prism.js or highlight.js
  return (
    <pre className="bg-muted/50 rounded-lg p-4 overflow-x-auto text-xs">
      <code className={cn('font-mono', language && `language-${language}`)}>
        {code}
      </code>
    </pre>
  );
};

export const DatasetPreview: React.FC<DatasetPreviewProps> = ({
  files,
  maxPreview = 20,
  onFileSelect,
  showContent = true,
  loading = false,
}) => {
  const [expandedRows, setExpandedRows] = useState<Set<string>>(new Set());

  const toggleRow = (fileId: string) => {
    setExpandedRows(prev => {
      const next = new Set(prev);
      if (next.has(fileId)) {
        next.delete(fileId);
      } else {
        next.add(fileId);
      }
      return next;
    });
  };

  const previewFiles = files.slice(0, maxPreview);
  const hasMore = files.length > maxPreview;

  if (loading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Dataset Preview</CardTitle>
          <CardDescription>Loading files...</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="space-y-2 animate-pulse">
            {[...Array(5)].map((_, i) => (
              <div key={i} className="h-12 bg-muted rounded" />
            ))}
          </div>
        </CardContent>
      </Card>
    );
  }

  if (files.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Dataset Preview</CardTitle>
          <CardDescription>No files to preview</CardDescription>
        </CardHeader>
        <CardContent className="flex items-center justify-center h-32 text-muted-foreground">
          <div className="text-center">
            <FileText className="h-12 w-12 mx-auto mb-2 opacity-50" />
            <p>No files in this dataset</p>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between">
          <span>Dataset Preview</span>
          <Badge variant="outline">
            {previewFiles.length} of {files.length} files
          </Badge>
        </CardTitle>
        <CardDescription>
          {hasMore
            ? `Showing first ${maxPreview} files. Total: ${files.length} files.`
            : `All ${files.length} files shown.`}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="rounded-md border">
          <Table>
            <TableHeader>
              <TableRow>
                {showContent && <TableHead className="w-[40px]"></TableHead>}
                <TableHead className="w-[40px]">Status</TableHead>
                <TableHead>Filename</TableHead>
                <TableHead>Language</TableHead>
                <TableHead className="text-right">Tokens</TableHead>
                <TableHead className="text-right">Size</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {previewFiles.map(file => {
                const isExpanded = expandedRows.has(file.id);
                return (
                  <React.Fragment key={file.id}>
                    <TableRow
                      className={cn(
                        'cursor-pointer',
                        onFileSelect && 'hover:bg-muted/50'
                      )}
                      onClick={() => onFileSelect?.(file)}
                    >
                      {showContent && (
                        <TableCell>
                          <Button
                            variant="ghost"
                            size="sm"
                            className="h-8 w-8 p-0"
                            onClick={e => {
                              e.stopPropagation();
                              toggleRow(file.id);
                            }}
                          >
                            {isExpanded ? (
                              <ChevronDown className="h-4 w-4" />
                            ) : (
                              <ChevronRight className="h-4 w-4" />
                            )}
                          </Button>
                        </TableCell>
                      )}
                      <TableCell>{getStatusIcon(file.status)}</TableCell>
                      <TableCell className="font-medium max-w-[300px] truncate">
                        {file.file_name}
                      </TableCell>
                      <TableCell>
                        {file.language && (
                          <Badge
                            variant="outline"
                            className={getLanguageBadgeColor(file.language)}
                          >
                            {file.language}
                          </Badge>
                        )}
                      </TableCell>
                      <TableCell className="text-right font-mono text-sm">
                        {file.tokens !== undefined ? file.tokens.toLocaleString() : '—'}
                      </TableCell>
                      <TableCell className="text-right text-sm text-muted-foreground">
                        {formatBytes(file.size_bytes)}
                      </TableCell>
                    </TableRow>
                    {showContent && isExpanded && (
                      <TableRow>
                        <TableCell colSpan={6} className="bg-muted/30 p-4">
                          <div className="space-y-3">
                            <div className="flex items-center justify-between text-sm">
                              <div className="space-y-1">
                                <div className="font-medium">File Details</div>
                                <div className="text-xs text-muted-foreground">
                                  Path: {file.file_path}
                                </div>
                                <div className="text-xs text-muted-foreground">
                                  Hash: {file.hash_b3.slice(0, 16)}...
                                </div>
                              </div>
                            </div>
                            {file.error && (
                              <div className="text-sm text-red-500 bg-red-500/10 border border-red-500/20 rounded-lg p-3">
                                Error: {file.error}
                              </div>
                            )}
                            {file.content && (
                              <div className="space-y-2">
                                <div className="text-sm font-medium">Content Preview</div>
                                <SyntaxHighlightedCode
                                  code={file.content.slice(0, 500) + (file.content.length > 500 ? '\n...' : '')}
                                  language={file.language}
                                />
                              </div>
                            )}
                          </div>
                        </TableCell>
                      </TableRow>
                    )}
                  </React.Fragment>
                );
              })}
            </TableBody>
          </Table>
        </div>

        {hasMore && (
          <div className="mt-4 text-center">
            <p className="text-sm text-muted-foreground">
              {files.length - maxPreview} more files not shown
            </p>
          </div>
        )}

        {/* Summary Statistics */}
        <div className="mt-6 grid grid-cols-2 md:grid-cols-4 gap-4">
          <div className="text-center p-3 bg-muted/50 rounded-lg">
            <div className="text-2xl font-bold text-green-500">
              {files.filter(f => f.status === 'success').length}
            </div>
            <div className="text-xs text-muted-foreground">Success</div>
          </div>
          <div className="text-center p-3 bg-muted/50 rounded-lg">
            <div className="text-2xl font-bold text-yellow-500">
              {files.filter(f => f.status === 'warning').length}
            </div>
            <div className="text-xs text-muted-foreground">Warnings</div>
          </div>
          <div className="text-center p-3 bg-muted/50 rounded-lg">
            <div className="text-2xl font-bold text-red-500">
              {files.filter(f => f.status === 'error').length}
            </div>
            <div className="text-xs text-muted-foreground">Errors</div>
          </div>
          <div className="text-center p-3 bg-muted/50 rounded-lg">
            <div className="text-2xl font-bold">
              {files.reduce((sum, f) => sum + (f.tokens || 0), 0).toLocaleString()}
            </div>
            <div className="text-xs text-muted-foreground">Total Tokens</div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
};

export default DatasetPreview;
