import React from 'react';
import { useParams } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { apiClient } from '@/api/services';
import { formatDate } from 'date-fns';

export default function AdapterUsage() {
  const { adapterId } = useParams<{ adapterId: string }>();

  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ['adapter-usage', adapterId],
    queryFn: async () => {
      if (!adapterId) throw new Error('Adapter ID required');
      return apiClient.getAdapterUsage(adapterId);
    },
    enabled: !!adapterId,
    refetchInterval: 10000, // Refresh every 10 seconds
  });

  if (isLoading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Adapter Usage Statistics</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-center h-32">
            <div className="text-muted-foreground">Loading usage statistics...</div>
          </div>
        </CardContent>
      </Card>
    );
  }

  if (error) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Adapter Usage Statistics</CardTitle>
        </CardHeader>
        <CardContent>
          {errorRecoveryTemplates.networkError(refetch)}
        </CardContent>
      </Card>
    );
  }

  if (!data) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Adapter Usage Statistics</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="text-muted-foreground">No usage data available</div>
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle>Usage Statistics for {adapterId}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="space-y-2">
              <div className="text-sm font-medium text-muted-foreground">Total Calls</div>
              <div className="text-3xl font-bold">{data.call_count}</div>
              <div className="text-xs text-muted-foreground">
                Number of times this adapter was selected
              </div>
            </div>
            <div className="space-y-2">
              <div className="text-sm font-medium text-muted-foreground">Average Confidence Score</div>
              <div className="text-3xl font-bold">{data.average_gate_value.toFixed(4)}</div>
              <div className="text-xs text-muted-foreground">
                Average confidence score when adapter was selected (0-1 range)
              </div>
            </div>
            <div className="space-y-2">
              <div className="text-sm font-medium text-muted-foreground">Last Used</div>
              <div className="text-lg font-semibold">
                {data.last_used ? (
                  formatDate(new Date(data.last_used), 'PPp')
                ) : (
                  <span className="text-muted-foreground">Never</span>
                )}
              </div>
              <div className="text-xs text-muted-foreground">
                Most recent activation timestamp
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      {data.call_count === 0 && (
        <Card>
          <CardContent className="pt-6">
            <div className="text-center text-muted-foreground">
              This adapter has not been used in any routing decisions yet.
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

