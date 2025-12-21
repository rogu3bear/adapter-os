import React, { useState, useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Accordion, AccordionItem, AccordionTrigger, AccordionContent } from '@/components/ui/accordion';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { AlertCircle, CheckCircle2, AlertTriangle, Filter, Download } from 'lucide-react';
import { PolicyCheckItem } from './PolicyCheckItem';
import { PolicyDetails } from './PolicyDetails';
import { PolicyOverride } from './PolicyOverride';
import { logger } from '@/utils/logger';

export type PolicyStatus = 'passed' | 'failed' | 'warning' | 'pending';
export type PolicyCategory = 'security' | 'quality' | 'compliance' | 'performance';
export type PolicySeverity = 'critical' | 'high' | 'medium' | 'low';

export interface PolicyCheck {
  id: string;
  name: string;
  description: string;
  status: PolicyStatus;
  category: PolicyCategory;
  severity: PolicySeverity;
  message?: string;
  remediation?: string;
  details?: {
    expectedValue?: string | number;
    actualValue?: string | number;
    threshold?: string | number;
    componentAffected?: string[];
  };
  documentationUrl?: string;
  canOverride?: boolean;
  overrideReason?: string;
}

export interface PolicyCheckDisplayProps {
  cpid: string;
  policies: PolicyCheck[];
  loading?: boolean;
  onOverride?: (policyId: string, reason: string) => Promise<void>;
  blockPromotion?: boolean;
  allowAdmin?: boolean;
  userRole?: string;
}

export type FilterStatus = 'all' | 'failed' | 'warnings';

export function PolicyCheckDisplay({
  cpid,
  policies,
  loading = false,
  onOverride,
  blockPromotion = false,
  allowAdmin = false,
  userRole = 'viewer',
}: PolicyCheckDisplayProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [filterStatus, setFilterStatus] = useState<FilterStatus>('all');
  const [expandedPolicies, setExpandedPolicies] = useState<Set<string>>(new Set());
  const [overridingPolicyId, setOverridingPolicyId] = useState<string | null>(null);
  const [overrideMessage, setOverrideMessage] = useState<string | null>(null);

  // Calculate statistics
  const stats = useMemo(() => {
    const total = policies.length;
    const passed = policies.filter(p => p.status === 'passed').length;
    const failed = policies.filter(p => p.status === 'failed').length;
    const warnings = policies.filter(p => p.status === 'warning').length;
    const passRate = total > 0 ? Math.round((passed / total) * 100) : 0;

    return { total, passed, failed, warnings, passRate };
  }, [policies]);

  // Filter policies
  const filteredPolicies = useMemo(() => {
    return policies.filter(policy => {
      // Status filter
      if (filterStatus === 'failed' && policy.status !== 'failed') return false;
      if (filterStatus === 'warnings' && !['warning', 'failed'].includes(policy.status)) return false;

      // Search query filter
      if (searchQuery) {
        const query = searchQuery.toLowerCase();
        return (
          policy.name.toLowerCase().includes(query) ||
          policy.description.toLowerCase().includes(query) ||
          (policy.message?.toLowerCase() || '').includes(query)
        );
      }

      return true;
    });
  }, [policies, searchQuery, filterStatus]);

  // Group by category
  const policiesByCategory = useMemo(() => {
    const grouped: Record<PolicyCategory, PolicyCheck[]> = {
      security: [],
      quality: [],
      compliance: [],
      performance: [],
    };

    filteredPolicies.forEach(policy => {
      grouped[policy.category].push(policy);
    });

    return grouped;
  }, [filteredPolicies]);

  const handleOverride = async (policyId: string, reason: string) => {
    try {
      if (onOverride) {
        setOverridingPolicyId(policyId);
        await onOverride(policyId, reason);
        setOverridingPolicyId(null);
        setOverrideMessage(`Policy "${policyId}" overridden successfully`);
        logger.info('Policy override applied', { cpid, policyId, reason });

        // Clear message after 3 seconds
        setTimeout(() => setOverrideMessage(null), 3000);
      }
    } catch (error) {
      logger.error('Policy override failed', { cpid, policyId }, error instanceof Error ? error : new Error(String(error)));
      setOverridingPolicyId(null);
    }
  };

  const exportReport = () => {
    const report = {
      cpid,
      exportedAt: new Date().toISOString(),
      summary: stats,
      policies: filteredPolicies.map(p => ({
        id: p.id,
        name: p.name,
        status: p.status,
        category: p.category,
        severity: p.severity,
        message: p.message,
        remediation: p.remediation,
      })),
    };

    const json = JSON.stringify(report, null, 2);
    const blob = new Blob([json], { type: 'application/json' });
    const url = window.URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `policy-check-${cpid}-${Date.now()}.json`;
    link.click();
    window.URL.revokeObjectURL(url);

    logger.info('Policy report exported', { cpid });
  };

  const getStatusBadgeVariant = (status: PolicyStatus) => {
    switch (status) {
      case 'passed':
        return 'success';
      case 'failed':
        return 'error';
      case 'warning':
        return 'warning';
      case 'pending':
        return 'info';
      default:
        return 'neutral';
    }
  };

  const getStatusIcon = (status: PolicyStatus) => {
    switch (status) {
      case 'passed':
        return <CheckCircle2 className="w-4 h-4" />;
      case 'failed':
        return <AlertCircle className="w-4 h-4" />;
      case 'warning':
        return <AlertTriangle className="w-4 h-4" />;
      default:
        return null;
    }
  };

  const categories: PolicyCategory[] = ['security', 'quality', 'compliance', 'performance'];

  // Determine if promotion should be blocked
  const shouldBlockPromotion = blockPromotion || stats.failed > 0;

  return (
    <div className="w-full space-y-4">
      {/* Override success message */}
      {overrideMessage && (
        <Alert variant="default" className="bg-success-surface border-success-border">
          <CheckCircle2 className="h-4 w-4 text-success" />
          <AlertDescription className="text-success">{overrideMessage}</AlertDescription>
        </Alert>
      )}

      {/* Critical failure alert */}
      {shouldBlockPromotion && (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertTitle>Promotion Blocked</AlertTitle>
          <AlertDescription>
            This plan cannot be promoted due to {stats.failed} critical policy failure{stats.failed !== 1 ? 's' : ''}.
            {allowAdmin && userRole?.toLowerCase() === 'admin' && ' Admins can override non-critical policies below.'}
          </AlertDescription>
        </Alert>
      )}

      {/* Summary Card */}
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <div>
              <CardTitle className="text-lg">Policy Validation</CardTitle>
              <p className="text-sm text-muted-foreground mt-1">
                Plan ID: <code className="bg-muted px-2 py-1 rounded text-xs">{cpid}</code>
              </p>
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={exportReport}
              className="gap-2"
              disabled={policies.length === 0}
            >
              <Download className="w-4 h-4" />
              Export Report
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-4 gap-4">
            <div className="space-y-2">
              <p className="text-sm font-medium text-muted-foreground">Total Policies</p>
              <p className="text-2xl font-bold">{stats.total}</p>
            </div>
            <div className="space-y-2">
              <p className="text-sm font-medium text-muted-foreground">Passing</p>
              <p className="text-2xl font-bold text-green-600">{stats.passed}</p>
            </div>
            <div className="space-y-2">
              <p className="text-sm font-medium text-muted-foreground">Warnings</p>
              <p className="text-2xl font-bold text-yellow-600">{stats.warnings}</p>
            </div>
            <div className="space-y-2">
              <p className="text-sm font-medium text-muted-foreground">Failed</p>
              <p className="text-2xl font-bold text-red-600">{stats.failed}</p>
            </div>
          </div>
          <div className="mt-4 pt-4 border-t">
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium">Pass Rate</span>
              <div className="flex items-center gap-2">
                <div className="w-32 bg-muted rounded-full h-2">
                  <div
                    className={`h-2 rounded-full transition-all ${
                      stats.passRate === 100
                        ? 'bg-green-500'
                        : stats.passRate >= 80
                          ? 'bg-yellow-500'
                          : 'bg-red-500'
                    }`}
                    style={{ width: `${stats.passRate}%` }}
                  />
                </div>
                <span className="text-sm font-medium">{stats.passRate}%</span>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Filter and Search */}
      <Card>
        <CardContent className="pt-6 space-y-3">
          <div className="flex gap-2">
            <Input
              placeholder="Search policies..."
              value={searchQuery}
              onChange={e => setSearchQuery(e.target.value)}
              className="flex-1"
            />
            <Button
              variant={filterStatus === 'all' ? 'default' : 'outline'}
              size="sm"
              onClick={() => setFilterStatus('all')}
              className="gap-2"
            >
              <Filter className="w-4 h-4" />
              All ({policies.length})
            </Button>
            <Button
              variant={filterStatus === 'warnings' ? 'default' : 'outline'}
              size="sm"
              onClick={() => setFilterStatus('warnings')}
            >
              Warnings ({stats.warnings})
            </Button>
            <Button
              variant={filterStatus === 'failed' ? 'default' : 'outline'}
              size="sm"
              onClick={() => setFilterStatus('failed')}
            >
              Failed ({stats.failed})
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Policies by Category */}
      {categories.map(category => {
        const categoryPolicies = policiesByCategory[category];
        if (categoryPolicies.length === 0) return null;

        const categoryFailures = categoryPolicies.filter(p => p.status === 'failed').length;
        const categoryWarnings = categoryPolicies.filter(p => p.status === 'warning').length;

        return (
          <Card key={category}>
            <CardHeader className="pb-3">
              <div className="flex items-center justify-between">
                <CardTitle className="text-base capitalize">{category} Policies</CardTitle>
                <div className="flex gap-1">
                  {categoryFailures > 0 && (
                    <Badge variant="error" className="gap-1">
                      <AlertCircle className="w-3 h-3" />
                      {categoryFailures} failed
                    </Badge>
                  )}
                  {categoryWarnings > 0 && (
                    <Badge variant="warning" className="gap-1">
                      <AlertTriangle className="w-3 h-3" />
                      {categoryWarnings} warnings
                    </Badge>
                  )}
                </div>
              </div>
            </CardHeader>
            <CardContent>
              <Accordion type="single" collapsible className="w-full">
                {categoryPolicies.map(policy => (
                  <AccordionItem key={policy.id} value={policy.id}>
                    <AccordionTrigger className="hover:no-underline py-2">
                      <PolicyCheckItem
                        policy={policy}
                        icon={getStatusIcon(policy.status)}
                      />
                    </AccordionTrigger>
                    <AccordionContent className="pt-4 space-y-3">
                      <PolicyDetails policy={policy} />

                      {/* Override section for admins */}
                      {allowAdmin && userRole?.toLowerCase() === 'admin' && policy.canOverride && (
                        <div className="pt-3 border-t">
                          <PolicyOverride
                            policyId={policy.id}
                            policyName={policy.name}
                            onOverride={handleOverride}
                            isLoading={overridingPolicyId === policy.id}
                            severity={policy.severity}
                          />
                        </div>
                      )}
                    </AccordionContent>
                  </AccordionItem>
                ))}
              </Accordion>
            </CardContent>
          </Card>
        );
      })}

      {/* Empty state */}
      {filteredPolicies.length === 0 && !loading && (
        <Card className="text-center py-8">
          <CardContent>
            <AlertCircle className="w-8 h-8 mx-auto mb-2 text-muted-foreground" />
            <p className="text-muted-foreground">
              {searchQuery ? 'No policies match your search' : 'No policies to display'}
            </p>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

export default PolicyCheckDisplay;
