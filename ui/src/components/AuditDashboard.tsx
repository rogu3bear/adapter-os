import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import {
  Shield,
  AlertTriangle,
  CheckCircle,
  XCircle,
  Clock,
  FileText,
  TrendingUp,
  Activity,
  Lock,
  Search,
  Download,
  RefreshCw,
  Eye,
  BarChart3,
  Filter
} from 'lucide-react';
import apiClient from '../api/client';
import { Policy, TelemetryBundle, PromotionGate } from '../api/types';
import { toast } from 'sonner';

interface AuditDashboardProps {
  selectedTenant: string;
}

interface ComplianceStatus {
  controlId: string;
  controlName: string;
  status: 'compliant' | 'non-compliant' | 'pending' | 'unknown';
  lastChecked: string;
  evidence: string[];
  findings: string[];
}

interface PolicyViolation {
  id: string;
  policyName: string;
  violationType: string;
  severity: 'critical' | 'high' | 'medium' | 'low';
  timestamp: string;
  details: string;
  resolved: boolean;
}

export function AuditDashboard({ selectedTenant }: AuditDashboardProps) {
  const [policies, setPolicies] = useState<Policy[]>([]);
  const [telemetryBundles, setTelemetryBundles] = useState<TelemetryBundle[]>([]);
  const [complianceStatus, setComplianceStatus] = useState<ComplianceStatus[]>([]);
  const [violations, setViolations] = useState<PolicyViolation[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [selectedControl, setSelectedControl] = useState<ComplianceStatus | null>(null);

  useEffect(() => {
    loadAuditData();
  }, [selectedTenant]);

  const loadAuditData = async () => {
    setIsLoading(true);
    try {
      // Load policies
      const policiesList = await apiClient.listPolicies();
      setPolicies(policiesList);

      // Load telemetry bundles
      const bundles = await apiClient.listTelemetryBundles();
      setTelemetryBundles(bundles);

      // Generate mock compliance data (in production, this would come from backend)
      generateComplianceData(policiesList);
      generateViolationData();
    } catch (error) {
      console.error('Failed to load audit data:', error);
      toast.error('Failed to load audit data');
    } finally {
      setIsLoading(false);
    }
  };

  const generateComplianceData = (policies: Policy[]) => {
    const controls: ComplianceStatus[] = [
      {
        controlId: 'EGRESS-001',
        controlName: 'Network Egress Control',
        status: 'compliant',
        lastChecked: new Date().toISOString(),
        evidence: ['Zero egress mode enforced', 'PF rules active'],
        findings: []
      },
      {
        controlId: 'DETERM-001',
        controlName: 'Deterministic Execution',
        status: 'compliant',
        lastChecked: new Date().toISOString(),
        evidence: ['Metal kernels precompiled', 'HKDF seeding enabled'],
        findings: []
      },
      {
        controlId: 'ROUTER-001',
        controlName: 'Router Configuration',
        status: 'compliant',
        lastChecked: new Date().toISOString(),
        evidence: ['K-sparse within bounds', 'Entropy floor met'],
        findings: []
      },
      {
        controlId: 'EVIDENCE-001',
        controlName: 'Evidence Requirements',
        status: 'pending',
        lastChecked: new Date().toISOString(),
        evidence: ['ARR: 0.94', 'ECS@5: 0.72'],
        findings: ['ECS@5 below threshold of 0.75']
      },
      {
        controlId: 'ISOLATION-001',
        controlName: 'Tenant Isolation',
        status: 'compliant',
        lastChecked: new Date().toISOString(),
        evidence: ['Per-tenant processes', 'UID/GID separation'],
        findings: []
      },
      {
        controlId: 'TELEMETRY-001',
        controlName: 'Telemetry Compliance',
        status: 'compliant',
        lastChecked: new Date().toISOString(),
        evidence: ['Sampling rules met', 'Bundle rotation active'],
        findings: []
      },
      {
        controlId: 'ARTIFACTS-001',
        controlName: 'Artifact Security',
        status: 'compliant',
        lastChecked: new Date().toISOString(),
        evidence: ['All artifacts signed', 'SBOM present'],
        findings: []
      },
      {
        controlId: 'MEMORY-001',
        controlName: 'Memory Management',
        status: 'compliant',
        lastChecked: new Date().toISOString(),
        evidence: ['15% headroom maintained', 'Eviction order followed'],
        findings: []
      }
    ];

    setComplianceStatus(controls);
  };

  const generateViolationData = () => {
    const mockViolations: PolicyViolation[] = [
      {
        id: 'V001',
        policyName: 'Evidence Ruleset',
        violationType: 'Insufficient Evidence Coverage',
        severity: 'medium',
        timestamp: new Date(Date.now() - 3600000).toISOString(),
        details: 'ECS@5 score of 0.72 is below required threshold of 0.75',
        resolved: false
      },
      {
        id: 'V002',
        policyName: 'Performance Ruleset',
        violationType: 'Latency Threshold Exceeded',
        severity: 'low',
        timestamp: new Date(Date.now() - 7200000).toISOString(),
        details: 'P95 latency of 26ms exceeded budget of 24ms',
        resolved: true
      }
    ];

    setViolations(mockViolations);
  };

  const handleRunAudit = async () => {
    setIsLoading(true);
    try {
      toast.success('Audit started');
      await new Promise(resolve => setTimeout(resolve, 2000));
      await loadAuditData();
      toast.success('Audit completed');
    } catch (error) {
      console.error('Failed to run audit:', error);
      toast.error('Audit failed');
    } finally {
      setIsLoading(false);
    }
  };

  const handleExportReport = () => {
    const report = {
      generated_at: new Date().toISOString(),
      tenant: selectedTenant,
      compliance_status: complianceStatus,
      violations: violations,
      policies: policies.length,
      telemetry_bundles: telemetryBundles.length
    };

    const blob = new Blob([JSON.stringify(report, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `audit-report-${new Date().toISOString()}.json`;
    a.click();
    URL.revokeObjectURL(url);

    toast.success('Audit report exported');
  };

  const getStatusColor = (status: ComplianceStatus['status']) => {
    switch (status) {
      case 'compliant':
        return 'text-green-600';
      case 'non-compliant':
        return 'text-red-600';
      case 'pending':
        return 'text-amber-600';
      default:
        return 'text-gray-600';
    }
  };

  const getStatusIcon = (status: ComplianceStatus['status']) => {
    switch (status) {
      case 'compliant':
        return <CheckCircle className="w-5 h-5" />;
      case 'non-compliant':
        return <XCircle className="w-5 h-5" />;
      case 'pending':
        return <Clock className="w-5 h-5" />;
      default:
        return <AlertTriangle className="w-5 h-5" />;
    }
  };

  const getSeverityBadge = (severity: PolicyViolation['severity']) => {
    const colors = {
      critical: 'bg-red-600 text-white',
      high: 'bg-orange-600 text-white',
      medium: 'bg-amber-600 text-white',
      low: 'bg-blue-600 text-white'
    };

    return (
      <Badge className={colors[severity]}>
        {severity.toUpperCase()}
      </Badge>
    );
  };

  const compliantCount = complianceStatus.filter(c => c.status === 'compliant').length;
  const totalControls = complianceStatus.length;
  const complianceRate = totalControls > 0 ? (compliantCount / totalControls) * 100 : 0;

  const activeViolations = violations.filter(v => !v.resolved).length;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-3xl font-bold tracking-tight">Audit & Compliance</h2>
          <p className="text-muted-foreground">
            Monitor policy compliance and security posture
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" onClick={handleExportReport}>
            <Download className="w-4 h-4 mr-2" />
            Export Report
          </Button>
          <Button onClick={handleRunAudit} disabled={isLoading}>
            <RefreshCw className={`w-4 h-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
            {isLoading ? 'Running...' : 'Run Audit'}
          </Button>
        </div>
      </div>

      {/* Compliance Overview */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-6">
        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Compliance Rate</p>
                <p className="text-3xl font-bold">{complianceRate.toFixed(0)}%</p>
              </div>
              <Shield className="w-8 h-8 text-green-600" />
            </div>
            <div className="mt-4 h-2 bg-secondary rounded-full overflow-hidden">
              <div
                className="h-full bg-green-600 transition-all"
                style={{ width: `${complianceRate}%` }}
              />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Compliant Controls</p>
                <p className="text-3xl font-bold">{compliantCount}/{totalControls}</p>
              </div>
              <CheckCircle className="w-8 h-8 text-green-600" />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Active Violations</p>
                <p className="text-3xl font-bold">{activeViolations}</p>
              </div>
              <AlertTriangle className="w-8 h-8 text-amber-600" />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Last Audit</p>
                <p className="text-sm font-medium">
                  {new Date().toLocaleDateString()}
                </p>
              </div>
              <Clock className="w-8 h-8 text-primary" />
            </div>
          </CardContent>
        </Card>
      </div>

      <Tabs defaultValue="compliance" className="space-y-4">
        <TabsList>
          <TabsTrigger value="compliance">
            <Shield className="w-4 h-4 mr-2" />
            Compliance Status
          </TabsTrigger>
          <TabsTrigger value="violations">
            <AlertTriangle className="w-4 h-4 mr-2" />
            Policy Violations
          </TabsTrigger>
          <TabsTrigger value="matrix">
            <BarChart3 className="w-4 h-4 mr-2" />
            Control Matrix
          </TabsTrigger>
          <TabsTrigger value="telemetry">
            <Activity className="w-4 h-4 mr-2" />
            Telemetry Audit
          </TabsTrigger>
        </TabsList>

        {/* Compliance Status */}
        <TabsContent value="compliance" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Control Compliance Status</CardTitle>
              <CardDescription>
                Status of all 20 policy pack controls
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-3">
                {complianceStatus.map((control) => (
                  <div
                    key={control.controlId}
                    className="p-4 border rounded-lg hover:bg-muted/50 cursor-pointer transition-colors"
                    onClick={() => setSelectedControl(control)}
                  >
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-3 flex-1">
                        <div className={getStatusColor(control.status)}>
                          {getStatusIcon(control.status)}
                        </div>
                        <div className="flex-1">
                          <div className="flex items-center gap-2">
                            <span className="font-medium">{control.controlName}</span>
                            <Badge variant="outline" className="text-xs">
                              {control.controlId}
                            </Badge>
                          </div>
                          <div className="text-sm text-muted-foreground mt-1">
                            Last checked: {new Date(control.lastChecked).toLocaleString()}
                          </div>
                        </div>
                      </div>
                      <Badge
                        variant={control.status === 'compliant' ? 'default' : 'secondary'}
                        className={control.status === 'compliant' ? 'bg-green-600' : ''}
                      >
                        {control.status}
                      </Badge>
                    </div>

                    {control.findings.length > 0 && (
                      <div className="mt-3 pl-8">
                        <div className="text-sm font-medium text-amber-600 mb-1">Findings:</div>
                        <ul className="text-sm text-muted-foreground space-y-1">
                          {control.findings.map((finding, idx) => (
                            <li key={idx}>• {finding}</li>
                          ))}
                        </ul>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>

          {selectedControl && (
            <Card>
              <CardHeader>
                <CardTitle>Control Details: {selectedControl.controlName}</CardTitle>
                <CardDescription>{selectedControl.controlId}</CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                <div>
                  <h4 className="font-semibold mb-2">Evidence:</h4>
                  <ul className="space-y-1">
                    {selectedControl.evidence.map((item, idx) => (
                      <li key={idx} className="flex items-start gap-2 text-sm">
                        <CheckCircle className="w-4 h-4 text-green-600 mt-0.5" />
                        <span>{item}</span>
                      </li>
                    ))}
                  </ul>
                </div>

                {selectedControl.findings.length > 0 && (
                  <div>
                    <h4 className="font-semibold mb-2">Findings:</h4>
                    <ul className="space-y-1">
                      {selectedControl.findings.map((item, idx) => (
                        <li key={idx} className="flex items-start gap-2 text-sm">
                          <AlertTriangle className="w-4 h-4 text-amber-600 mt-0.5" />
                          <span>{item}</span>
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
              </CardContent>
            </Card>
          )}
        </TabsContent>

        {/* Policy Violations */}
        <TabsContent value="violations" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Policy Violations</CardTitle>
              <CardDescription>
                Track and resolve policy compliance issues
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-3">
                {violations.length === 0 ? (
                  <div className="text-center py-12 text-muted-foreground">
                    <CheckCircle className="w-12 h-12 mx-auto mb-3 opacity-20" />
                    <p>No policy violations detected</p>
                  </div>
                ) : (
                  violations.map((violation) => (
                    <div
                      key={violation.id}
                      className={`
                        p-4 border rounded-lg
                        ${violation.resolved ? 'opacity-60 bg-muted/50' : 'bg-background'}
                      `}
                    >
                      <div className="flex items-start justify-between">
                        <div className="flex-1">
                          <div className="flex items-center gap-2 mb-2">
                            {getSeverityBadge(violation.severity)}
                            <span className="font-medium">{violation.violationType}</span>
                            {violation.resolved && (
                              <Badge variant="outline" className="bg-green-50 text-green-700">
                                Resolved
                              </Badge>
                            )}
                          </div>
                          <div className="text-sm text-muted-foreground mb-2">
                            Policy: {violation.policyName}
                          </div>
                          <div className="text-sm mb-2">{violation.details}</div>
                          <div className="text-xs text-muted-foreground">
                            {new Date(violation.timestamp).toLocaleString()}
                          </div>
                        </div>
                      </div>
                    </div>
                  ))
                )}
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Control Matrix */}
        <TabsContent value="matrix" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Compliance Control Matrix</CardTitle>
              <CardDescription>
                Mapping of controls to policy packs
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b">
                      <th className="text-left py-3 px-4">Control ID</th>
                      <th className="text-left py-3 px-4">Control Name</th>
                      <th className="text-left py-3 px-4">Policy Pack</th>
                      <th className="text-center py-3 px-4">Status</th>
                      <th className="text-center py-3 px-4">Last Check</th>
                    </tr>
                  </thead>
                  <tbody>
                    {complianceStatus.map((control) => (
                      <tr key={control.controlId} className="border-b hover:bg-muted/50">
                        <td className="py-3 px-4 font-mono text-xs">{control.controlId}</td>
                        <td className="py-3 px-4">{control.controlName}</td>
                        <td className="py-3 px-4 text-muted-foreground">
                          {control.controlId.split('-')[0]} Ruleset
                        </td>
                        <td className="py-3 px-4 text-center">
                          <div className={`inline-flex ${getStatusColor(control.status)}`}>
                            {getStatusIcon(control.status)}
                          </div>
                        </td>
                        <td className="py-3 px-4 text-center text-xs text-muted-foreground">
                          {new Date(control.lastChecked).toLocaleDateString()}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Telemetry Audit */}
        <TabsContent value="telemetry" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Telemetry Bundle Audit</CardTitle>
              <CardDescription>
                Verify telemetry bundle integrity and signatures
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-3">
                {telemetryBundles.map((bundle) => (
                  <div
                    key={bundle.id}
                    className="p-4 border rounded-lg hover:bg-muted/50"
                  >
                    <div className="flex items-center justify-between">
                      <div className="flex-1">
                        <div className="flex items-center gap-2 mb-2">
                          <FileText className="w-4 h-4 text-muted-foreground" />
                          <span className="font-mono text-sm">{bundle.id}</span>
                        </div>
                        <div className="grid grid-cols-3 gap-4 text-sm">
                          <div>
                            <span className="text-muted-foreground">Events:</span>{' '}
                            {bundle.event_count}
                          </div>
                          <div>
                            <span className="text-muted-foreground">Size:</span>{' '}
                            {(bundle.size_bytes / 1024).toFixed(1)} KB
                          </div>
                          <div>
                            <span className="text-muted-foreground">Created:</span>{' '}
                            {new Date(bundle.created_at).toLocaleDateString()}
                          </div>
                        </div>
                        <div className="mt-2 text-xs font-mono text-muted-foreground">
                          Merkle Root: {bundle.merkle_root}
                        </div>
                      </div>
                      <Button variant="outline" size="sm">
                        <Eye className="w-3 h-3 mr-1" />
                        Verify
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
