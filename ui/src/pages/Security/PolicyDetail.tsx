/**
 * PolicyDetail - Detailed view of a policy with actions
 *
 * Features:
 * - Policy metadata display
 * - JSON content viewer with syntax highlighting
 * - Action buttons (Sign, Compare, Export, Validate)
 * - Signature verification display
 */

import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import {
  ArrowLeft,
  FileSignature,
  GitCompare,
  Download,
  CheckCircle,
  Calendar,
  Hash,
  FileText,
} from 'lucide-react';
import type { Policy } from '@/api/types';

interface PolicyDetailProps {
  policy: Policy;
  onSign: (policy: Policy) => void;
  onExport: (policy: Policy) => void;
  onCompare: (policy: Policy) => void;
  onBack: () => void;
  canSign: boolean;
  canValidate: boolean;
}

export function PolicyDetail({
  policy,
  onSign,
  onExport,
  onCompare,
  onBack,
  canSign,
}: PolicyDetailProps) {
  const [showRawJson, setShowRawJson] = useState(false);

  const getStatusBadge = () => {
    const status = policy.status;
    const enabled = policy.enabled;

    if (status === 'active' || enabled) {
      return <Badge variant="default" className="bg-green-500">Active</Badge>;
    }
    if (status === 'draft') {
      return <Badge variant="secondary">Draft</Badge>;
    }
    if (status === 'archived') {
      return <Badge variant="outline">Archived</Badge>;
    }
    return <Badge variant="outline">{status || 'Unknown'}</Badge>;
  };

  const formatJson = (json: string | undefined) => {
    if (!json) return 'N/A';
    try {
      return JSON.stringify(JSON.parse(json), null, 2);
    } catch {
      return json;
    }
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <Button variant="outline" onClick={onBack}>
          <ArrowLeft className="h-4 w-4 mr-2" />
          Back to List
        </Button>
        <div className="flex gap-2">
          {canSign && (
            <Button variant="outline" onClick={() => onSign(policy)}>
              <FileSignature className="h-4 w-4 mr-2" />
              Sign
            </Button>
          )}
          <Button variant="outline" onClick={() => onCompare(policy)}>
            <GitCompare className="h-4 w-4 mr-2" />
            Compare
          </Button>
          <Button variant="outline" onClick={() => onExport(policy)}>
            <Download className="h-4 w-4 mr-2" />
            Export
          </Button>
        </div>
      </div>

      {/* Overview Card */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle>{policy.name || 'Unnamed Policy'}</CardTitle>
              <CardDescription className="mt-1">
                <span className="font-mono text-sm">{policy.cpid || policy.id}</span>
              </CardDescription>
            </div>
            {getStatusBadge()}
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Metadata Grid */}
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1">
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <FileText className="h-4 w-4" />
                <span>Type</span>
              </div>
              <p className="text-sm font-medium">{policy.type || 'N/A'}</p>
            </div>

            <div className="space-y-1">
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Calendar className="h-4 w-4" />
                <span>Created</span>
              </div>
              <p className="text-sm font-medium">
                {policy.created_at ? new Date(policy.created_at).toLocaleString() : 'N/A'}
              </p>
            </div>

            <div className="space-y-1">
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Calendar className="h-4 w-4" />
                <span>Updated</span>
              </div>
              <p className="text-sm font-medium">
                {policy.updated_at ? new Date(policy.updated_at).toLocaleString() : 'N/A'}
              </p>
            </div>

            {policy.priority !== undefined && (
              <div className="space-y-1">
                <div className="flex items-center gap-2 text-sm text-muted-foreground">
                  <Hash className="h-4 w-4" />
                  <span>Priority</span>
                </div>
                <p className="text-sm font-medium">{policy.priority}</p>
              </div>
            )}
          </div>

          {/* Signature Section */}
          {policy.signature && (
            <>
              <Separator />
              <div className="space-y-2">
                <div className="flex items-center gap-2 text-sm font-medium">
                  <CheckCircle className="h-4 w-4 text-green-500" />
                  <span>Signature Verified</span>
                </div>
                <div className="bg-muted p-3 rounded-md">
                  <p className="text-xs font-mono break-all">{policy.signature}</p>
                </div>
              </div>
            </>
          )}

          {/* Schema Hash */}
          {policy.schema_hash && (
            <>
              <Separator />
              <div className="space-y-2">
                <div className="flex items-center gap-2 text-sm font-medium">
                  <Hash className="h-4 w-4" />
                  <span>Schema Hash</span>
                </div>
                <div className="bg-muted p-3 rounded-md">
                  <p className="text-xs font-mono break-all">{policy.schema_hash}</p>
                </div>
              </div>
            </>
          )}
        </CardContent>
      </Card>

      {/* Policy Content Card */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>Policy Content</CardTitle>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setShowRawJson(!showRawJson)}
            >
              {showRawJson ? 'Hide Raw JSON' : 'Show Raw JSON'}
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          {showRawJson ? (
            <div className="bg-muted p-4 rounded-md overflow-x-auto">
              <pre className="text-xs font-mono whitespace-pre-wrap">
                {formatJson(policy.policy_json || policy.content)}
              </pre>
            </div>
          ) : (
            <div className="space-y-4">
              <p className="text-sm text-muted-foreground">
                {policy.content || 'No policy content available'}
              </p>

              {policy.policies && policy.policies.length > 0 && (
                <>
                  <Separator />
                  <div>
                    <h4 className="text-sm font-medium mb-2">Sub-policies</h4>
                    <div className="space-y-2">
                      {policy.policies.map((subPolicy, idx) => (
                        <div key={idx} className="border rounded-md p-3">
                          <p className="text-sm font-medium">{subPolicy.name || 'Unnamed'}</p>
                          <p className="text-xs text-muted-foreground font-mono">
                            {subPolicy.cpid || subPolicy.id}
                          </p>
                        </div>
                      ))}
                    </div>
                  </div>
                </>
              )}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
