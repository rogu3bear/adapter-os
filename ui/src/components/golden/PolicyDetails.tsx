import React from 'react';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { AlertCircle, CheckCircle2, AlertTriangle, BookOpen } from 'lucide-react';
import { PolicyCheck } from './PolicyCheckDisplay';

export interface PolicyDetailsProps {
  policy: PolicyCheck;
}

function PolicyDetailsComponent({ policy }: PolicyDetailsProps) {
  const getAlertVariant = (status: string) => {
    switch (status) {
      case 'failed':
        return 'destructive';
      default:
        return 'default';
    }
  };

  const getAlertIcon = (status: string) => {
    switch (status) {
      case 'failed':
        return <AlertCircle className="h-4 w-4" />;
      case 'warning':
        return <AlertTriangle className="h-4 w-4" />;
      case 'passed':
        return <CheckCircle2 className="h-4 w-4 text-green-600" />;
      default:
        return null;
    }
  };

  return (
    <div className="space-y-3">
      {/* Main message/alert */}
      {policy.message && (
        <Alert variant={getAlertVariant(policy.status)}>
          {getAlertIcon(policy.status)}
          <AlertTitle>{policy.status === 'passed' ? 'Validation Passed' : 'Validation Details'}</AlertTitle>
          <AlertDescription>{policy.message}</AlertDescription>
        </Alert>
      )}

      {/* Details section */}
      {policy.details && (
        <Card className="bg-muted/50">
          <CardContent className="pt-4 space-y-2">
            <div className="text-sm font-semibold mb-3">Validation Details</div>

            {policy.details.expectedValue !== undefined && (
              <div className="flex justify-between items-start gap-2">
                <span className="text-xs text-muted-foreground font-medium">Expected:</span>
                <code className="text-xs bg-background px-2 py-1 rounded font-mono">
                  {String(policy.details.expectedValue)}
                </code>
              </div>
            )}

            {policy.details.actualValue !== undefined && (
              <div className="flex justify-between items-start gap-2">
                <span className="text-xs text-muted-foreground font-medium">Actual:</span>
                <code className="text-xs bg-background px-2 py-1 rounded font-mono">
                  {String(policy.details.actualValue)}
                </code>
              </div>
            )}

            {policy.details.threshold !== undefined && (
              <div className="flex justify-between items-start gap-2">
                <span className="text-xs text-muted-foreground font-medium">Threshold:</span>
                <code className="text-xs bg-background px-2 py-1 rounded font-mono">
                  {String(policy.details.threshold)}
                </code>
              </div>
            )}

            {policy.details.componentAffected && policy.details.componentAffected.length > 0 && (
              <div className="flex justify-between items-start gap-2">
                <span className="text-xs text-muted-foreground font-medium">Components:</span>
                <div className="flex flex-wrap gap-1 justify-end">
                  {policy.details.componentAffected.map((comp, idx) => (
                    <code
                      key={idx}
                      className="text-xs bg-background px-2 py-1 rounded font-mono inline-block"
                    >
                      {comp}
                    </code>
                  ))}
                </div>
              </div>
            )}
          </CardContent>
        </Card>
      )}

      {/* Remediation section */}
      {policy.remediation && policy.status !== 'passed' && (
        <div className="border-l-2 border-yellow-500 pl-3 py-2">
          <div className="text-xs font-semibold text-yellow-700 mb-1">How to Fix</div>
          <p className="text-xs text-muted-foreground leading-relaxed">{policy.remediation}</p>
        </div>
      )}

      {/* Documentation link */}
      {policy.documentationUrl && (
        <div className="pt-2">
          <Button
            variant="outline"
            size="sm"
            className="w-full gap-2 text-xs"
            onClick={() => window.open(policy.documentationUrl, '_blank')}
          >
            <BookOpen className="w-3 h-3" />
            View Documentation
          </Button>
        </div>
      )}

      {/* Success message */}
      {policy.status === 'passed' && (
        <Alert variant="default" className="bg-green-50 border-green-200">
          <CheckCircle2 className="h-4 w-4 text-green-600" />
          <AlertTitle className="text-green-900">All Checks Passed</AlertTitle>
          <AlertDescription className="text-green-700">
            This policy is fully compliant and ready for deployment.
          </AlertDescription>
        </Alert>
      )}
    </div>
  );
}

export const PolicyDetails = React.memo(PolicyDetailsComponent);
