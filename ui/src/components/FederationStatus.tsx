/**
 * Federation Status Component
 * 
 * Displays federation verification status and quarantine state.
 * 
 * Features:
 * - Real-time federation status display
 * - Quarantine alerts and management
 * - Host chain verification overview
 * - Release quarantine action (admin only)
 */

import React, { useState, useEffect } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import { AlertCircle, CheckCircle2, XCircle, ShieldAlert, Unlock } from 'lucide-react';
import { useToast } from '@/hooks/use-toast';

interface FederationVerificationReport {
  ok: boolean;
  hosts_verified: number;
  errors: string[];
  verified_at: string;
}

interface FederationStatusResponse {
  operational: boolean;
  quarantined: boolean;
  quarantine_reason?: string;
  latest_verification?: FederationVerificationReport;
  total_hosts: number;
  timestamp: string;
}

interface QuarantineDetails {
  reason: string;
  triggered_at: string;
  violation_type: string;
  cpid?: string;
}

interface QuarantineStatusResponse {
  quarantined: boolean;
  details?: QuarantineDetails;
}

const API_BASE = '/api/v1';

async function fetchFederationStatus(): Promise<FederationStatusResponse> {
  const response = await fetch(`${API_BASE}/federation/status`, {
    credentials: 'include',
  });
  
  if (!response.ok) {
    throw new Error('Failed to fetch federation status');
  }
  
  return response.json();
}

async function fetchQuarantineStatus(): Promise<QuarantineStatusResponse> {
  const response = await fetch(`${API_BASE}/federation/quarantine`, {
    credentials: 'include',
  });
  
  if (!response.ok) {
    throw new Error('Failed to fetch quarantine status');
  }
  
  return response.json();
}

async function releaseQuarantine(): Promise<void> {
  const response = await fetch(`${API_BASE}/federation/release-quarantine`, {
    method: 'POST',
    credentials: 'include',
  });
  
  if (!response.ok) {
    throw new Error('Failed to release quarantine');
  }
}

export function FederationStatus() {
  const { toast } = useToast();
  const queryClient = useQueryClient();

  // Fetch federation status
  const { data: status, isLoading, error, refetch } = useQuery({
    queryKey: ['federation-status'],
    queryFn: fetchFederationStatus,
    refetchInterval: 10000, // Refetch every 10 seconds
  });

  // Fetch quarantine details
  const { data: quarantineStatus } = useQuery({
    queryKey: ['quarantine-status'],
    queryFn: fetchQuarantineStatus,
    refetchInterval: 10000,
    enabled: status?.quarantined || false,
  });

  // Release quarantine mutation
  const releaseMutation = useMutation({
    mutationFn: releaseQuarantine,
    onSuccess: () => {
      toast({
        title: 'Quarantine Released',
        description: 'System has been released from quarantine.',
      });
      queryClient.invalidateQueries({ queryKey: ['federation-status'] });
      queryClient.invalidateQueries({ queryKey: ['quarantine-status'] });
    },
    onError: (error: Error) => {
      toast({
        title: 'Error',
        description: `Failed to release quarantine: ${error.message}`,
        variant: 'destructive',
      });
    },
  });

  const handleReleaseQuarantine = () => {
    if (confirm('Are you sure you want to release the system from quarantine?')) {
      releaseMutation.mutate();
    }
  };

  if (isLoading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Federation Status</CardTitle>
          <CardDescription>Loading...</CardDescription>
        </CardHeader>
      </Card>
    );
  }

  if (error) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Federation Status</CardTitle>
          <CardDescription>Error loading status</CardDescription>
        </CardHeader>
        <CardContent>
          <Alert variant="destructive">
            <AlertCircle className="h-4 w-4" />
            <AlertTitle>Error</AlertTitle>
            <AlertDescription>
              {error instanceof Error ? error.message : 'Unknown error occurred'}
            </AlertDescription>
          </Alert>
        </CardContent>
      </Card>
    );
  }

  const getStatusIcon = () => {
    if (status?.quarantined) {
      return <ShieldAlert className="h-5 w-5 text-red-500" />;
    }
    if (status?.operational) {
      return <CheckCircle2 className="h-5 w-5 text-green-500" />;
    }
    return <XCircle className="h-5 w-5 text-yellow-500" />;
  };

  const getStatusBadge = () => {
    if (status?.quarantined) {
      return <Badge variant="destructive">QUARANTINED</Badge>;
    }
    if (status?.operational) {
      return <Badge variant="default" className="bg-green-600">OPERATIONAL</Badge>;
    }
    return <Badge variant="secondary">DEGRADED</Badge>;
  };

  return (
    <Card className="w-full">
      <CardHeader>
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            {getStatusIcon()}
            <div>
              <CardTitle>Federation Chain</CardTitle>
              <CardDescription>
                Cross-host verification status
              </CardDescription>
            </div>
          </div>
          {getStatusBadge()}
        </div>
      </CardHeader>
      
      <CardContent className="space-y-4">
        {/* Quarantine Alert */}
        {status?.quarantined && (
          <Alert variant="destructive">
            <ShieldAlert className="h-4 w-4" />
            <AlertTitle>System Quarantined</AlertTitle>
            <AlertDescription className="mt-2 space-y-2">
              <p>{status.quarantine_reason}</p>
              {quarantineStatus?.details && (
                <>
                  <Separator className="my-2" />
                  <div className="text-sm space-y-1">
                    <p><strong>Violation Type:</strong> {quarantineStatus.details.violation_type}</p>
                    <p><strong>Triggered:</strong> {new Date(quarantineStatus.details.triggered_at).toLocaleString()}</p>
                    {quarantineStatus.details.cpid && (
                      <p><strong>CPID:</strong> {quarantineStatus.details.cpid}</p>
                    )}
                  </div>
                </>
              )}
              <div className="mt-3">
                <Button 
                  variant="outline" 
                  size="sm"
                  onClick={handleReleaseQuarantine}
                  disabled={releaseMutation.isPending}
                >
                  <Unlock className="h-4 w-4 mr-2" />
                  {releaseMutation.isPending ? 'Releasing...' : 'Release Quarantine'}
                </Button>
              </div>
            </AlertDescription>
          </Alert>
        )}

        {/* Verification Report */}
        {status?.latest_verification && (
          <div className="space-y-3">
            <h3 className="text-sm font-semibold">Latest Verification</h3>
            <div className="grid grid-cols-2 gap-4 text-sm">
              <div>
                <p className="text-muted-foreground">Status</p>
                <p className="font-medium">
                  {status.latest_verification.ok ? (
                    <span className="text-green-600 flex items-center gap-1">
                      <CheckCircle2 className="h-4 w-4" />
                      Verified
                    </span>
                  ) : (
                    <span className="text-red-600 flex items-center gap-1">
                      <XCircle className="h-4 w-4" />
                      Failed
                    </span>
                  )}
                </p>
              </div>
              <div>
                <p className="text-muted-foreground">Hosts Verified</p>
                <p className="font-medium">
                  {status.latest_verification.hosts_verified} / {status.total_hosts}
                </p>
              </div>
              <div className="col-span-2">
                <p className="text-muted-foreground">Last Verified</p>
                <p className="font-medium">
                  {new Date(status.latest_verification.verified_at).toLocaleString()}
                </p>
              </div>
            </div>

            {/* Errors */}
            {status.latest_verification.errors.length > 0 && (
              <div className="space-y-2">
                <Separator />
                <h4 className="text-sm font-semibold text-red-600">Verification Errors</h4>
                <div className="space-y-1">
                  {status.latest_verification.errors.map((error, index) => (
                    <div key={index} className="text-sm text-muted-foreground bg-red-50 dark:bg-red-950/20 p-2 rounded">
                      {error}
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}

        {/* Summary Stats */}
        <Separator />
        <div className="grid grid-cols-3 gap-4 text-sm">
          <div>
            <p className="text-muted-foreground">Total Hosts</p>
            <p className="text-2xl font-bold">{status?.total_hosts || 0}</p>
          </div>
          <div>
            <p className="text-muted-foreground">Status</p>
            <p className="text-lg font-semibold">
              {status?.operational ? 'Operational' : 'Degraded'}
            </p>
          </div>
          <div>
            <p className="text-muted-foreground">Updated</p>
            <p className="text-xs">
              {status?.timestamp ? new Date(status.timestamp).toLocaleString() : 'N/A'}
            </p>
          </div>
        </div>

        {/* Manual Refresh */}
        <div className="flex justify-end">
          <Button 
            variant="outline" 
            size="sm" 
            onClick={() => refetch()}
          >
            Refresh Status
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

export default FederationStatus;

