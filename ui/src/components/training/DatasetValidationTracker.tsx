import React from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import { Progress } from '../ui/progress';
import { Alert, AlertDescription } from '../ui/alert';
import { Checkbox } from '../ui/checkbox';
import {
  CheckCircle2,
  XCircle,
  AlertCircle,
  Loader2,
  FileCheck,
  Hash,
  Copy,
  Languages,
  Shield,
} from 'lucide-react';
import { cn } from '../ui/utils';

interface ValidationCheck {
  id: string;
  label: string;
  description: string;
  status: 'pending' | 'running' | 'passed' | 'failed' | 'warning';
  message?: string;
  icon?: React.ComponentType<{ className?: string }>;
}

interface DatasetValidationTrackerProps {
  checks?: ValidationCheck[];
  overallStatus?: 'pending' | 'validating' | 'valid' | 'invalid';
  progress?: number;
  errors?: string[];
  warnings?: string[];
  onRetry?: () => void;
}

const getStatusIcon = (status: ValidationCheck['status'], IconComponent?: React.ComponentType<{ className?: string }>) => {
  const Icon = IconComponent || FileCheck;

  switch (status) {
    case 'passed':
      return <CheckCircle2 className="h-5 w-5 text-green-500" />;
    case 'failed':
      return <XCircle className="h-5 w-5 text-red-500" />;
    case 'warning':
      return <AlertCircle className="h-5 w-5 text-yellow-500" />;
    case 'running':
      return <Loader2 className="h-5 w-5 text-blue-500 animate-spin" />;
    default:
      return <Icon className="h-5 w-5 text-muted-foreground" />;
  }
};

const getStatusBadge = (status: ValidationCheck['status']) => {
  switch (status) {
    case 'passed':
      return <Badge className="bg-green-500/10 text-green-500 border-green-500/20">Passed</Badge>;
    case 'failed':
      return <Badge variant="destructive">Failed</Badge>;
    case 'warning':
      return <Badge className="bg-yellow-500/10 text-yellow-500 border-yellow-500/20">Warning</Badge>;
    case 'running':
      return <Badge className="bg-blue-500/10 text-blue-500 border-blue-500/20">Running</Badge>;
    default:
      return <Badge variant="outline">Pending</Badge>;
  }
};

const DEFAULT_CHECKS: ValidationCheck[] = [
  {
    id: 'files_parsed',
    label: 'All files parsed successfully',
    description: 'Verify all uploaded files can be read and processed',
    status: 'pending',
    icon: FileCheck,
  },
  {
    id: 'token_limits',
    label: 'Token count within limits',
    description: 'Check that total tokens and per-file tokens are within acceptable ranges',
    status: 'pending',
    icon: Hash,
  },
  {
    id: 'no_duplicates',
    label: 'No duplicate documents',
    description: 'Ensure no duplicate files based on content hash',
    status: 'pending',
    icon: Copy,
  },
  {
    id: 'min_dataset_size',
    label: 'Minimum dataset size met',
    description: 'Dataset contains at least 10 examples for training',
    status: 'pending',
    icon: Shield,
  },
  {
    id: 'language_consistency',
    label: 'Language consistency checked',
    description: 'Verify primary language matches expected distribution',
    status: 'pending',
    icon: Languages,
  },
];

export const DatasetValidationTracker: React.FC<DatasetValidationTrackerProps> = ({
  checks = DEFAULT_CHECKS,
  overallStatus = 'pending',
  progress = 0,
  errors = [],
  warnings = [],
  onRetry,
}) => {
  const passedCount = checks.filter(c => c.status === 'passed').length;
  const failedCount = checks.filter(c => c.status === 'failed').length;
  const warningCount = checks.filter(c => c.status === 'warning').length;
  const runningCount = checks.filter(c => c.status === 'running').length;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between">
          <span className="flex items-center gap-2">
            {overallStatus === 'validating' && <Loader2 className="h-5 w-5 animate-spin" />}
            {overallStatus === 'valid' && <CheckCircle2 className="h-5 w-5 text-green-500" />}
            {overallStatus === 'invalid' && <XCircle className="h-5 w-5 text-red-500" />}
            Dataset Validation
          </span>
          <div className="flex gap-2">
            {passedCount > 0 && (
              <Badge className="bg-green-500/10 text-green-500 border-green-500/20">
                {passedCount} passed
              </Badge>
            )}
            {warningCount > 0 && (
              <Badge className="bg-yellow-500/10 text-yellow-500 border-yellow-500/20">
                {warningCount} warnings
              </Badge>
            )}
            {failedCount > 0 && (
              <Badge variant="destructive">
                {failedCount} failed
              </Badge>
            )}
          </div>
        </CardTitle>
        <CardDescription>
          {overallStatus === 'validating' && 'Validating dataset...'}
          {overallStatus === 'valid' && 'All validation checks passed'}
          {overallStatus === 'invalid' && 'Some validation checks failed'}
          {overallStatus === 'pending' && 'Ready to validate'}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        {/* Progress Bar */}
        {overallStatus === 'validating' && (
          <div className="space-y-2">
            <div className="flex justify-between text-sm">
              <span>Validation Progress</span>
              <span className="text-muted-foreground">{Math.round(progress)}%</span>
            </div>
            <Progress value={progress} className="h-2" />
          </div>
        )}

        {/* Validation Checklist */}
        <div className="space-y-3">
          <div className="text-sm font-medium">Validation Checklist</div>
          <div className="space-y-2">
            {checks.map(check => (
              <div
                key={check.id}
                className={cn(
                  'flex items-start gap-3 p-3 rounded-lg border transition-colors',
                  check.status === 'passed' && 'bg-green-500/5 border-green-500/20',
                  check.status === 'failed' && 'bg-red-500/5 border-red-500/20',
                  check.status === 'warning' && 'bg-yellow-500/5 border-yellow-500/20',
                  check.status === 'running' && 'bg-blue-500/5 border-blue-500/20'
                )}
              >
                <div className="mt-0.5">
                  {getStatusIcon(check.status, check.icon)}
                </div>
                <div className="flex-1 space-y-1">
                  <div className="flex items-center justify-between">
                    <div className="font-medium text-sm">{check.label}</div>
                    {getStatusBadge(check.status)}
                  </div>
                  <div className="text-xs text-muted-foreground">
                    {check.description}
                  </div>
                  {check.message && (
                    <div className={cn(
                      'text-xs mt-2 p-2 rounded',
                      check.status === 'failed' && 'bg-red-500/10 text-red-500',
                      check.status === 'warning' && 'bg-yellow-500/10 text-yellow-500',
                      check.status === 'passed' && 'bg-green-500/10 text-green-500'
                    )}>
                      {check.message}
                    </div>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Overall Status Alert */}
        {overallStatus === 'valid' && (
          <Alert className="border-green-500/20 bg-green-500/10">
            <CheckCircle2 className="h-4 w-4 text-green-500" />
            <AlertDescription className="text-green-500">
              Dataset validation completed successfully. All checks passed.
            </AlertDescription>
          </Alert>
        )}

        {overallStatus === 'invalid' && errors.length > 0 && (
          <Alert variant="destructive">
            <XCircle className="h-4 w-4" />
            <AlertDescription>
              <div className="font-medium mb-2">Validation failed with {errors.length} error(s):</div>
              <ul className="list-disc list-inside space-y-1">
                {errors.slice(0, 5).map((error, i) => (
                  <li key={i} className="text-sm">{error}</li>
                ))}
              </ul>
              {errors.length > 5 && (
                <div className="text-sm mt-2">
                  ... and {errors.length - 5} more errors
                </div>
              )}
            </AlertDescription>
          </Alert>
        )}

        {warnings.length > 0 && (
          <Alert className="border-yellow-500/20 bg-yellow-500/10">
            <AlertCircle className="h-4 w-4 text-yellow-500" />
            <AlertDescription className="text-yellow-500">
              <div className="font-medium mb-2">{warnings.length} warning(s):</div>
              <ul className="list-disc list-inside space-y-1">
                {warnings.slice(0, 3).map((warning, i) => (
                  <li key={i} className="text-sm">{warning}</li>
                ))}
              </ul>
              {warnings.length > 3 && (
                <div className="text-sm mt-2">
                  ... and {warnings.length - 3} more warnings
                </div>
              )}
            </AlertDescription>
          </Alert>
        )}

        {/* Validation Summary */}
        <div className="border-t pt-4">
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
            <div className="text-center p-3 bg-muted/50 rounded-lg">
              <div className="text-2xl font-bold text-green-500">{passedCount}</div>
              <div className="text-xs text-muted-foreground">Passed</div>
            </div>
            <div className="text-center p-3 bg-muted/50 rounded-lg">
              <div className="text-2xl font-bold text-yellow-500">{warningCount}</div>
              <div className="text-xs text-muted-foreground">Warnings</div>
            </div>
            <div className="text-center p-3 bg-muted/50 rounded-lg">
              <div className="text-2xl font-bold text-red-500">{failedCount}</div>
              <div className="text-xs text-muted-foreground">Failed</div>
            </div>
            <div className="text-center p-3 bg-muted/50 rounded-lg">
              <div className="text-2xl font-bold">{checks.length}</div>
              <div className="text-xs text-muted-foreground">Total Checks</div>
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
};

export default DatasetValidationTracker;
