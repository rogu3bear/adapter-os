import React, { useState } from 'react';
import { Button } from '../ui/button';
import { Textarea } from '../ui/textarea';
import { Alert, AlertDescription, AlertTitle } from '../ui/alert';
import { Card, CardContent } from '../ui/card';
import { AlertTriangle, CheckCircle2, Lock } from 'lucide-react';

export interface PolicyOverrideProps {
  policyId: string;
  policyName: string;
  severity: string;
  onOverride: (policyId: string, reason: string) => Promise<void>;
  isLoading?: boolean;
}

export function PolicyOverride({
  policyId,
  policyName,
  severity,
  onOverride,
  isLoading = false,
}: PolicyOverrideProps) {
  const [overrideReason, setOverrideReason] = useState('');
  const [showForm, setShowForm] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async () => {
    if (!overrideReason.trim()) {
      setError('Please provide a reason for overriding this policy');
      return;
    }

    if (overrideReason.trim().length < 20) {
      setError('Please provide a detailed reason (at least 20 characters)');
      return;
    }

    try {
      setIsSubmitting(true);
      setError(null);
      await onOverride(policyId, overrideReason);
      setShowForm(false);
      setOverrideReason('');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to override policy');
    } finally {
      setIsSubmitting(false);
    }
  };

  const isCritical = severity === 'critical';

  if (!showForm) {
    return (
      <div className="pt-2">
        <Button
          variant="outline"
          size="sm"
          className="w-full gap-2"
          onClick={() => setShowForm(true)}
          disabled={isLoading}
        >
          <Lock className="w-3 h-3" />
          {isCritical ? 'Request Override' : 'Override Policy'}
        </Button>
      </div>
    );
  }

  return (
    <Card className="bg-amber-50 border-amber-200">
      <CardContent className="pt-4 space-y-3">
        <Alert variant="default" className="bg-amber-100 border-amber-300">
          <AlertTriangle className="h-4 w-4 text-amber-800" />
          <AlertTitle className="text-amber-900">Policy Override</AlertTitle>
          <AlertDescription className="text-amber-800 text-xs">
            {isCritical
              ? 'This is a critical policy. Overrides require justification and audit logging.'
              : 'Overriding this policy will bypass its validation checks.'}
          </AlertDescription>
        </Alert>

        <div className="space-y-2">
          <label className="text-xs font-semibold text-foreground">
            Reason for Override{isCritical && ' (Required)'}
          </label>
          <Textarea
            placeholder={`Explain why you need to override the "${policyName}" policy. Include business justification, risk assessment, and any mitigations.`}
            value={overrideReason}
            onChange={e => {
              setOverrideReason(e.target.value);
              setError(null);
            }}
            className="min-h-24 text-xs resize-none"
            disabled={isSubmitting}
          />
          <div className="text-xs text-muted-foreground">
            {overrideReason.length} / 20 characters minimum
          </div>
        </div>

        {error && (
          <Alert variant="destructive">
            <AlertTriangle className="h-3 w-3" />
            <AlertDescription className="text-xs">{error}</AlertDescription>
          </Alert>
        )}

        <div className="flex gap-2 pt-2">
          <Button
            variant="default"
            size="sm"
            onClick={handleSubmit}
            disabled={isSubmitting || !overrideReason.trim()}
            className="flex-1 gap-2"
          >
            {isSubmitting ? (
              <>
                <div className="w-3 h-3 border-2 border-white border-t-transparent rounded-full animate-spin" />
                Submitting...
              </>
            ) : (
              <>
                <CheckCircle2 className="w-3 h-3" />
                Confirm Override
              </>
            )}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              setShowForm(false);
              setOverrideReason('');
              setError(null);
            }}
            disabled={isSubmitting}
            className="flex-1"
          >
            Cancel
          </Button>
        </div>

        {isCritical && (
          <div className="text-xs bg-white p-2 rounded border border-amber-200 space-y-1">
            <p className="font-semibold text-amber-900">Audit Trail Note:</p>
            <ul className="list-disc list-inside text-amber-800 space-y-0.5">
              <li>Override will be logged with your user ID and timestamp</li>
              <li>Compliance team will review this decision</li>
              <li>All related artifacts will be marked for audit</li>
            </ul>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
