import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { Badge } from '../ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '../ui/table';
import {
  Copy,
  Download,
  AlertCircle,
  CheckCircle,
  ChevronDown,
  ChevronRight,
  RefreshCw,
  FileJson,
  FileSpreadsheet
} from 'lucide-react';
import { BatchInferItemResponse } from '../../api/types';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '../ui/collapsible';
import { logger } from '../../utils/logger';

interface BatchResultsProps {
  results: BatchInferItemResponse[];
  prompts: string[];
  onRetry?: (itemId: string) => void;
  onExportJSON?: () => void;
  onExportCSV?: () => void;
}

export function BatchResults({
  results,
  prompts,
  onRetry,
  onExportJSON,
  onExportCSV
}: BatchResultsProps) {
  const [expandedRows, setExpandedRows] = useState<Set<string>>(new Set());

  const toggleRow = (id: string) => {
    setExpandedRows(prev => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
    logger.info('Copied to clipboard', {
      component: 'BatchResults',
      operation: 'copy',
    });
  };

  const getStatusBadge = (result: BatchInferItemResponse) => {
    if (result.error) {
      return <Badge variant="destructive"><AlertCircle className="h-3 w-3 mr-1" />Error</Badge>;
    }
    if (result.response) {
      return <Badge variant="default"><CheckCircle className="h-3 w-3 mr-1" />Success</Badge>;
    }
    return <Badge variant="outline">Pending</Badge>;
  };

  const getPromptForResult = (result: BatchInferItemResponse): string => {
    const index = results.findIndex(r => r.id === result.id);
    return prompts[index] || result.id;
  };

  const truncate = (text: string, maxLength: number = 50): string => {
    if (text.length <= maxLength) return text;
    return text.substring(0, maxLength) + '...';
  };

  const successCount = results.filter(r => r.response).length;
  const errorCount = results.filter(r => r.error).length;
  const pendingCount = results.length - successCount - errorCount;

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <div>
            <CardTitle className="text-base">Batch Results</CardTitle>
            <div className="flex gap-2 mt-2">
              <Badge variant="default" className="gap-1">
                <CheckCircle className="h-3 w-3" />
                {successCount} Completed
              </Badge>
              {errorCount > 0 && (
                <Badge variant="destructive" className="gap-1">
                  <AlertCircle className="h-3 w-3" />
                  {errorCount} Errors
                </Badge>
              )}
              {pendingCount > 0 && (
                <Badge variant="outline" className="gap-1">
                  {pendingCount} Pending
                </Badge>
              )}
            </div>
          </div>
          <div className="flex gap-2">
            {onExportJSON && (
              <Button variant="outline" size="sm" onClick={onExportJSON}>
                <FileJson className="h-4 w-4 mr-2" />
                Export JSON
              </Button>
            )}
            {onExportCSV && (
              <Button variant="outline" size="sm" onClick={onExportCSV}>
                <FileSpreadsheet className="h-4 w-4 mr-2" />
                Export CSV
              </Button>
            )}
          </div>
        </div>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-[50px]"></TableHead>
              <TableHead className="w-[100px]">ID</TableHead>
              <TableHead>Prompt</TableHead>
              <TableHead className="w-[120px]">Status</TableHead>
              <TableHead>Response</TableHead>
              <TableHead className="w-[100px]">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {results.map((result) => {
              const isExpanded = expandedRows.has(result.id);
              const prompt = getPromptForResult(result);

              return (
                <React.Fragment key={result.id}>
                  <TableRow>
                    <TableCell>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => toggleRow(result.id)}
                        className="h-6 w-6 p-0"
                      >
                        {isExpanded ? (
                          <ChevronDown className="h-4 w-4" />
                        ) : (
                          <ChevronRight className="h-4 w-4" />
                        )}
                      </Button>
                    </TableCell>
                    <TableCell className="font-mono text-xs">{result.id}</TableCell>
                    <TableCell className="text-sm">{truncate(prompt, 60)}</TableCell>
                    <TableCell>{getStatusBadge(result)}</TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {result.response ? truncate(result.response, 80) :
                       result.error ? truncate(result.error, 80) :
                       '-'}
                    </TableCell>
                    <TableCell>
                      <div className="flex gap-1">
                        {result.response && (
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => handleCopy(result.response!)}
                            className="h-7 px-2"
                          >
                            <Copy className="h-3 w-3" />
                          </Button>
                        )}
                        {result.error && onRetry && (
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => onRetry(result.id)}
                            className="h-7 px-2"
                          >
                            <RefreshCw className="h-3 w-3" />
                          </Button>
                        )}
                      </div>
                    </TableCell>
                  </TableRow>
                  {isExpanded && (
                    <TableRow>
                      <TableCell colSpan={6} className="bg-muted/50">
                        <div className="p-4 space-y-4">
                          <div>
                            <div className="text-sm font-medium mb-2">Full Prompt:</div>
                            <pre className="text-xs bg-background p-3 rounded border overflow-x-auto">
                              {prompt}
                            </pre>
                          </div>
                          {result.response && (
                            <>
                              <div>
                                <div className="text-sm font-medium mb-2">Full Response:</div>
                                <pre className="text-xs bg-background p-3 rounded border overflow-x-auto whitespace-pre-wrap">
                                  {result.response}
                                </pre>
                              </div>
                              <div className="grid grid-cols-3 gap-4 text-sm">
                                <div>
                                  <span className="font-medium">Token Count:</span>{' '}
                                  {result.tokens || 0}
                                </div>
                                <div>
                                  <span className="font-medium">Latency:</span>{' '}
                                  {result.latency_ms || 0}ms
                                </div>
                              </div>
                            </>
                          )}
                          {result.error && (
                            <div>
                              <div className="text-sm font-medium mb-2 text-destructive">Error Details:</div>
                              <pre className="text-xs bg-destructive/10 p-3 rounded border border-destructive overflow-x-auto">
                                {JSON.stringify(result.error, null, 2)}
                              </pre>
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
      </CardContent>
    </Card>
  );
}
