/**
 * SecurityPage - Main security management page
 *
 * Features:
 * - Three tabs: Policies, Audit Logs, Compliance
 * - RBAC-aware access control
 * - Comprehensive security management interface
 */

import React, { useState } from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Shield, FileText, ClipboardCheck, Wrench, CheckSquare } from 'lucide-react';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { PageHeader } from '@/components/ui/page-header';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { PoliciesTab } from './PoliciesTab';
import { AuditLogsTab } from './AuditLogsTab';
import { ComplianceTab } from './ComplianceTab';
import PolicyStudio from './PolicyStudio';
import PolicyReviewQueue from './PolicyReviewQueue';

export default function SecurityPage() {
  const { can, hasRole } = useRBAC();
  const [activeTab, setActiveTab] = useState('policies');

  // Check if user has any security permissions
  const canViewPolicies = can('policy:view');
  const canViewAudit = hasRole(['admin', 'sre', 'compliance']);
  const canViewCompliance = hasRole(['admin', 'compliance']);
  const canCustomizePolicies = can('policy:customize');
  const canReviewPolicies = can('policy:review');

  // If user has no security permissions at all
  if (!canViewPolicies && !canViewAudit && !canViewCompliance && !canCustomizePolicies && !canReviewPolicies) {
    return (
      <div className="container mx-auto p-6">
        <PageHeader
          title="Security"
          description="Manage security policies, audit logs, and compliance"
        />
        <ErrorRecovery
          error="You do not have permission to access security features. This page requires admin, SRE, or compliance role."
          onRetry={() => window.location.reload()}
        />
      </div>
    );
  }

  // Determine default tab based on permissions
  React.useEffect(() => {
    if (activeTab === 'policies' && !canViewPolicies) {
      if (canViewAudit) {
        setActiveTab('audit');
      } else if (canViewCompliance) {
        setActiveTab('compliance');
      }
    }
  }, [activeTab, canViewPolicies, canViewAudit, canViewCompliance]);

  return (
    <div className="container mx-auto p-6 space-y-6">
      <PageHeader
        title="Security"
        description="Manage security policies, audit logs, and compliance controls"
      />

      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList className="grid w-full grid-cols-5 lg:w-auto">
          {canViewPolicies && (
            <TabsTrigger value="policies" className="flex items-center gap-2">
              <Shield className="h-4 w-4" />
              <span className="hidden sm:inline">Policies</span>
            </TabsTrigger>
          )}
          {canCustomizePolicies && (
            <TabsTrigger value="studio" className="flex items-center gap-2">
              <Wrench className="h-4 w-4" />
              <span className="hidden sm:inline">Policy Studio</span>
            </TabsTrigger>
          )}
          {canReviewPolicies && (
            <TabsTrigger value="review" className="flex items-center gap-2">
              <CheckSquare className="h-4 w-4" />
              <span className="hidden sm:inline">Review Queue</span>
            </TabsTrigger>
          )}
          {canViewAudit && (
            <TabsTrigger value="audit" className="flex items-center gap-2">
              <FileText className="h-4 w-4" />
              <span className="hidden sm:inline">Audit Logs</span>
            </TabsTrigger>
          )}
          {canViewCompliance && (
            <TabsTrigger value="compliance" className="flex items-center gap-2">
              <ClipboardCheck className="h-4 w-4" />
              <span className="hidden sm:inline">Compliance</span>
            </TabsTrigger>
          )}
        </TabsList>

        {canViewPolicies && (
          <TabsContent value="policies" className="mt-6">
            <SectionErrorBoundary sectionName="Policies">
              <PoliciesTab />
            </SectionErrorBoundary>
          </TabsContent>
        )}

        {canCustomizePolicies && (
          <TabsContent value="studio" className="mt-6">
            <SectionErrorBoundary sectionName="Policy Studio">
              <PolicyStudio />
            </SectionErrorBoundary>
          </TabsContent>
        )}

        {canReviewPolicies && (
          <TabsContent value="review" className="mt-6">
            <SectionErrorBoundary sectionName="Review Queue">
              <PolicyReviewQueue />
            </SectionErrorBoundary>
          </TabsContent>
        )}

        {canViewAudit && (
          <TabsContent value="audit" className="mt-6">
            <SectionErrorBoundary sectionName="Audit Logs">
              <AuditLogsTab />
            </SectionErrorBoundary>
          </TabsContent>
        )}

        {canViewCompliance && (
          <TabsContent value="compliance" className="mt-6">
            <SectionErrorBoundary sectionName="Compliance">
              <ComplianceTab />
            </SectionErrorBoundary>
          </TabsContent>
        )}

        {/* Fallback for disabled tabs */}
        {!canViewPolicies && activeTab === 'policies' && (
          <TabsContent value="policies" className="mt-6">
            <Card>
              <CardContent className="p-6">
                <ErrorRecovery
                  error="You do not have permission to view policies."
                  onRetry={() => setActiveTab('audit')}
                />
              </CardContent>
            </Card>
          </TabsContent>
        )}
      </Tabs>
    </div>
  );
}
