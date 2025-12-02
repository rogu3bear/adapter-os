// DatasetValidation - Validation tab for dataset detail page

import React, { useState, useEffect } from 'react';
import { useLiveData } from '@/hooks/useLiveData';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Progress } from '@/components/ui/progress';
import { CheckCircle, XCircle, RefreshCw, AlertCircle } from 'lucide-react';
import type { Dataset } from '@/api/training-types';

interface DatasetProgressEvent {
  dataset_id: string;
  event_type: string;
  current_file?: string;
  percentage_complete: number;
  total_files?: number;
  files_processed?: number;
  message: string;
  timestamp: string;
}

interface DatasetValidationProps {
  dataset: Dataset;
  onValidate: () => void;
  isValidating: boolean;
}

const STATUS_CONFIG: Record<string, { icon: React.ElementType; className: string; label: string }> = {
  draft: {
    icon: RefreshCw,
    className: 'text-yellow-500',
    label: 'Draft',
  },
  validating: {
    icon: RefreshCw,
    className: 'text-blue-500 animate-spin',
    label: 'Validating',
  },
  valid: {
    icon: CheckCircle,
    className: 'text-green-500',
    label: 'Valid',
  },
  invalid: {
    icon: XCircle,
    className: 'text-red-500',
    label: 'Invalid',
  },
  failed: {
    icon: AlertCircle,
    className: 'text-red-500',
    label: 'Failed',
  },
};

export default function DatasetValidation({ dataset, onValidate, isValidating }: DatasetValidationProps) {
  const [validationProgress, setValidationProgress] = useState<{
    percentage: number;
    currentFile?: string;
    message: string;
  } | null>(null);

  // Track polling cycle for progress estimation
  const pollCountRef = React.useRef(0);

  // Subscribe to validation progress events with polling fallback
  useLiveData({
    sseEndpoint: `/v1/datasets/upload/progress?dataset_id=${dataset.id}`,
    sseEventType: 'validation',
    fetchFn: async () => {
      // Polling fallback: fetch dataset status when SSE unavailable
      try {
        const response = await fetch(`/v1/datasets/${dataset.id}`);
        if (!response.ok) return null;
        const data = await response.json();

        // Check validation status and provide appropriate progress
        if (data.validation_status === 'validating') {
          // Increment poll count for time-based progress estimation
          pollCountRef.current += 1;
          // Estimate progress: starts at 10%, increases 5% per poll, caps at 90%
          const estimatedProgress = Math.min(10 + pollCountRef.current * 5, 90);

          return {
            dataset_id: dataset.id,
            event_type: 'validation',
            percentage_complete: estimatedProgress,
            message: `Validating dataset (${data.example_count || 0} examples)...`,
          } as DatasetProgressEvent;
        } else if (data.validation_status === 'valid') {
          // Reset poll counter and return completed
          pollCountRef.current = 0;
          return {
            dataset_id: dataset.id,
            event_type: 'validation',
            percentage_complete: 100,
            message: 'Validation complete',
          } as DatasetProgressEvent;
        } else if (data.validation_status === 'invalid') {
          // Validation failed
          pollCountRef.current = 0;
          return {
            dataset_id: dataset.id,
            event_type: 'validation',
            percentage_complete: 100,
            message: data.validation_errors || 'Validation failed',
          } as DatasetProgressEvent;
        }
      } catch {
        // Ignore fetch errors - SSE is primary
      }
      return null;
    },
    enabled: isValidating || dataset.validation_status === 'validating',
    pollingSpeed: 'fast',
    onSSEMessage: (event) => {
      const progressEvent = event as DatasetProgressEvent;
      if (progressEvent.event_type === 'validation') {
        setValidationProgress({
          percentage: progressEvent.percentage_complete,
          currentFile: progressEvent.current_file,
          message: progressEvent.message,
        });
      }
    },
  });

  // Clear progress when validation completes
  useEffect(() => {
    if (dataset.validation_status !== 'validating' && !isValidating) {
      setValidationProgress(null);
    }
  }, [dataset.validation_status, isValidating]);

  const statusConfig = STATUS_CONFIG[dataset.validation_status] || STATUS_CONFIG.draft;
  const StatusIcon = statusConfig.icon;

  interface ParsedError {
    file?: string;
    message: string;
    line?: number;
    column?: number;
  }

  const parseValidationErrors = (errors?: string): ParsedError[] => {
    if (!errors) return [];
    
    // Try to parse structured errors (file name, line, column)
    const errorPatterns = [
      /File\s+([^\s:]+):\s*Invalid\s+JSON\s+at\s+line\s+(\d+),\s*column\s+(\d+):\s*(.+)/i,
      /File\s+([^\s:]+):\s*Invalid\s+JSON\s+at\s+line\s+(\d+):\s*(.+)/i,
      /File\s+([^\s:]+)\s+is\s+empty\s+\(size:\s*(\d+)\s+bytes\)/i,
      /File\s+([^\s:]+)\s+has\s+invalid\s+UTF-8\s+encoding/i,
      /File\s+([^\s:]+):\s*(.+)/i,
    ];

    return errors.split('; ').filter(Boolean).map(error => {
      for (const pattern of errorPatterns) {
        const match = error.match(pattern);
        if (match) {
          if (match.length === 5) {
            // Full match with file, line, column, message
            return {
              file: match[1],
              line: parseInt(match[2], 10),
              column: parseInt(match[3], 10),
              message: match[4],
            };
          } else if (match.length === 4 && match[2].match(/^\d+$/)) {
            // Match with file, line, message
            return {
              file: match[1],
              line: parseInt(match[2], 10),
              message: match[3],
            };
          } else if (match.length >= 3) {
            // Match with file and message
            return {
              file: match[1],
              message: match.slice(2).join(' '),
            };
          }
        }
      }
      // Fallback: return as-is
      return { message: error };
    });
  };

  const errors = parseValidationErrors(dataset.validation_errors);

  return (
    <div className="space-y-6">
      {/* Validation Status */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <StatusIcon className={`h-5 w-5 ${statusConfig.className}`} />
            Validation Status
          </CardTitle>
          <CardDescription>
            Current validation status: {statusConfig.label}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            <div className="flex items-center gap-4">
              <Badge variant="outline" className="gap-1">
                <StatusIcon className={`h-3 w-3 ${statusConfig.className}`} />
                <span>{statusConfig.label}</span>
              </Badge>
              {(dataset.validation_status === 'draft' || dataset.validation_status === 'invalid') && (
                <Button onClick={onValidate} disabled={isValidating}>
                  <RefreshCw className={`h-4 w-4 mr-2 ${isValidating ? 'animate-spin' : ''}`} />
                  {isValidating ? 'Validating...' : 'Validate Dataset'}
                </Button>
              )}
            </div>
            {(isValidating || validationProgress) && (
              <div className="space-y-2">
                {validationProgress && (
                  <>
                    <div className="flex items-center justify-between text-sm">
                      <span className="text-muted-foreground">{validationProgress.message}</span>
                      <span className="font-medium">{Math.round(validationProgress.percentage)}%</span>
                    </div>
                    <Progress value={validationProgress.percentage} />
                    {validationProgress.currentFile && (
                      <p className="text-xs text-muted-foreground">
                        Processing: {validationProgress.currentFile}
                      </p>
                    )}
                  </>
                )}
                {!validationProgress && isValidating && (
                  <div className="text-sm text-muted-foreground">
                    Starting validation...
                  </div>
                )}
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Validation Errors */}
      {errors.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <XCircle className="h-5 w-5 text-red-500" />
              Validation Errors ({errors.length})
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-2">
              {errors.map((error, idx) => (
                <Alert key={idx} variant="destructive">
                  <AlertCircle className="h-4 w-4" />
                  <AlertDescription>
                    {error.file && (
                      <div className="font-semibold mb-1">
                        File: {error.file}
                        {error.line && (
                          <span className="text-muted-foreground ml-2">
                            (Line {error.line}
                            {error.column && `, Column ${error.column}`})
                          </span>
                        )}
                      </div>
                    )}
                    <div>{error.message}</div>
                  </AlertDescription>
                </Alert>
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Validation Success */}
      {dataset.validation_status === 'valid' && errors.length === 0 && (
        <Card>
          <CardContent className="pt-6">
            <Alert>
              <CheckCircle className="h-4 w-4" />
              <AlertDescription>
                Dataset validation passed. This dataset is ready for training.
              </AlertDescription>
            </Alert>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

