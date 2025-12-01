import React, { useState, useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Shield, Lock, Eye, AlertTriangle, CheckCircle, Settings, Save, Loader2 } from 'lucide-react';
import { usePolicies, usePolicyMutations } from '@/hooks/useSecurity';
import { toast } from 'sonner';
import type { Policy } from '@/api/adapter-types';
import { logger, toError } from '@/utils/logger';

interface PolicyRule {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  severity: 'low' | 'medium' | 'high' | 'critical';
  category: 'access' | 'data' | 'network' | 'crypto';
  cpid?: string;
}

// Helper function to map API Policy to PolicyRule
function mapPolicyToRule(policy: Policy): PolicyRule {
  // Derive category from policy type or name
  let category: PolicyRule['category'] = 'access';
  const typeLower = policy.type.toLowerCase();
  const nameLower = policy.name.toLowerCase();

  if (typeLower.includes('network') || nameLower.includes('egress') || nameLower.includes('network')) {
    category = 'network';
  } else if (typeLower.includes('crypto') || nameLower.includes('determinism') || nameLower.includes('entropy') || nameLower.includes('eddsa') || nameLower.includes('jwt')) {
    category = 'crypto';
  } else if (typeLower.includes('data') || nameLower.includes('audit') || nameLower.includes('logging')) {
    category = 'data';
  } else if (typeLower.includes('access') || nameLower.includes('isolation') || nameLower.includes('tenant')) {
    category = 'access';
  }

  // Derive severity from policy priority or type
  let severity: PolicyRule['severity'] = 'medium';
  if (policy.priority !== undefined) {
    if (policy.priority >= 90) severity = 'critical';
    else if (policy.priority >= 70) severity = 'high';
    else if (policy.priority >= 40) severity = 'medium';
    else severity = 'low';
  } else if (nameLower.includes('critical') || category === 'network' || nameLower.includes('isolation')) {
    severity = 'critical';
  } else if (nameLower.includes('high') || category === 'crypto') {
    severity = 'high';
  }

  // Extract description from content or policy_json
  let description = '';
  try {
    if (policy.policy_json) {
      const parsed = JSON.parse(policy.policy_json);
      description = parsed.description || policy.content?.substring(0, 100) || `${policy.type} policy`;
    } else {
      description = policy.content?.substring(0, 100) || `${policy.type} policy`;
    }
  } catch {
    description = policy.content?.substring(0, 100) || `${policy.type} policy`;
  }

  return {
    id: policy.id,
    name: policy.name,
    description,
    enabled: policy.enabled ?? policy.status === 'active',
    severity,
    category,
    cpid: policy.cpid,
  };
}

export default function SecurityPolicyEditor() {
  const { policies: apiPolicies, isLoading, error, refetch } = usePolicies();
  const { applyPolicy, isApplyingPolicy } = usePolicyMutations();
  const [selectedCategory, setSelectedCategory] = useState<string>('all');
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(false);
  const [localChanges, setLocalChanges] = useState<Record<string, Partial<PolicyRule>>>({});

  // Map API policies to PolicyRule format
  const policies = useMemo(() => {
    if (!apiPolicies) return [];
    return apiPolicies.map(mapPolicyToRule).map(policy => ({
      ...policy,
      ...localChanges[policy.id],
    }));
  }, [apiPolicies, localChanges]);

  const updatePolicy = (id: string, updates: Partial<PolicyRule>) => {
    setLocalChanges(prev => ({
      ...prev,
      [id]: { ...prev[id], ...updates },
    }));
    setHasUnsavedChanges(true);
  };

  const handleSavePolicies = async () => {
    try {
      // Apply all changed policies
      const changedPolicies = Object.entries(localChanges).filter(([_, changes]) => changes.enabled !== undefined);

      for (const [policyId, changes] of changedPolicies) {
        const policy = apiPolicies?.find(p => p.id === policyId);
        if (policy && policy.cpid) {
          // Update the policy content to reflect enabled state
          const updatedContent = {
            ...JSON.parse(policy.policy_json || policy.content || '{}'),
            enabled: changes.enabled,
          };

          await applyPolicy({
            cpid: policy.cpid,
            content: JSON.stringify(updatedContent),
          });
        }
      }

      toast.success('Policies updated successfully');
      setHasUnsavedChanges(false);
      setLocalChanges({});
      refetch();
    } catch (err) {
      toast.error('Failed to update policies');
      logger.error('Security policy update failed', {
        component: 'SecurityPolicyEditor',
        operation: 'updatePolicy',
        errorType: 'policy_update_failure',
        details: 'Failed to update security policy configuration'
      }, toError(err));
    }
  };

  const filteredPolicies = selectedCategory === 'all'
    ? policies
    : policies.filter(policy => policy.category === selectedCategory);

  const getSeverityColor = (severity: string) => {
    switch (severity) {
      case 'critical': return 'text-red-600 bg-red-50 border-red-200';
      case 'high': return 'text-orange-600 bg-orange-50 border-orange-200';
      case 'medium': return 'text-yellow-600 bg-yellow-50 border-yellow-200';
      case 'low': return 'text-green-600 bg-green-50 border-green-200';
      default: return 'text-gray-600 bg-gray-50 border-gray-200';
    }
  };

  const getSeverityIcon = (severity: string) => {
    switch (severity) {
      case 'critical': return <AlertTriangle className="h-4 w-4" />;
      case 'high': return <AlertTriangle className="h-4 w-4" />;
      case 'medium': return <Eye className="h-4 w-4" />;
      case 'low': return <CheckCircle className="h-4 w-4" />;
      default: return <Shield className="h-4 w-4" />;
    }
  };

  const getCategoryIcon = (category: string) => {
    switch (category) {
      case 'access': return <Lock className="h-4 w-4" />;
      case 'data': return <Eye className="h-4 w-4" />;
      case 'network': return <Shield className="h-4 w-4" />;
      case 'crypto': return <Settings className="h-4 w-4" />;
      default: return <Shield className="h-4 w-4" />;
    }
  };

  // Loading state
  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-center space-y-4">
          <Loader2 className="h-8 w-8 animate-spin mx-auto text-muted-foreground" />
          <p className="text-sm text-muted-foreground">Loading security policies...</p>
        </div>
      </div>
    );
  }

  // Error state
  if (error) {
    return (
      <div className="flex items-center justify-center h-full">
        <Card className="max-w-md">
          <CardHeader>
            <CardTitle className="flex items-center space-x-2 text-red-600">
              <AlertTriangle className="h-5 w-5" />
              <span>Error Loading Policies</span>
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground mb-4">
              Unable to load security policies. Please try again.
            </p>
            <Button onClick={() => refetch()} variant="outline">
              Retry
            </Button>
          </CardContent>
        </Card>
      </div>
    );
  }

  // Empty state
  if (!policies || policies.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <Card className="max-w-md">
          <CardHeader>
            <CardTitle className="flex items-center space-x-2">
              <Shield className="h-5 w-5 text-blue-500" />
              <span>No Policies Found</span>
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground">
              No security policies are currently configured in the system.
            </p>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6 h-full">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold flex items-center space-x-2">
            <Shield className="h-5 w-5 text-red-500" />
            <span>Security Policy Editor</span>
          </h2>
          <p className="text-sm text-muted-foreground">Configure AdapterOS security policies</p>
        </div>
        <div className="flex items-center space-x-2">
          {hasUnsavedChanges && (
            <Badge variant="outline" className="text-orange-600">
              Unsaved Changes
            </Badge>
          )}
          <Button onClick={handleSavePolicies} disabled={!hasUnsavedChanges || isApplyingPolicy}>
            {isApplyingPolicy ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                Saving...
              </>
            ) : (
              <>
                <Save className="h-4 w-4 mr-2" />
                Save Policies
              </>
            )}
          </Button>
        </div>
      </div>

      {/* Policy Overview */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <Card>
          <CardContent className="p-4">
            <div className="flex items-center space-x-2">
              <CheckCircle className="h-5 w-5 text-green-500" />
              <div>
                <p className="text-2xl font-bold text-green-600">
                  {policies.filter(p => p.enabled).length}
                </p>
                <p className="text-xs text-muted-foreground">Active Policies</p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="p-4">
            <div className="flex items-center space-x-2">
              <AlertTriangle className="h-5 w-5 text-red-500" />
              <div>
                <p className="text-2xl font-bold text-red-600">
                  {policies.filter(p => p.severity === 'critical' && p.enabled).length}
                </p>
                <p className="text-xs text-muted-foreground">Critical Policies</p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="p-4">
            <div className="flex items-center space-x-2">
              <Shield className="h-5 w-5 text-blue-500" />
              <div>
                <p className="text-2xl font-bold text-blue-600">
                  {policies.length}
                </p>
                <p className="text-xs text-muted-foreground">Total Policies</p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="p-4">
            <div className="flex items-center space-x-2">
              <Settings className="h-5 w-5 text-purple-500" />
              <div>
                <p className="text-2xl font-bold text-purple-600">4</p>
                <p className="text-xs text-muted-foreground">Categories</p>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Policy Configuration */}
      <Tabs defaultValue="policies" className="h-full">
        <div className="flex items-center justify-between">
          <TabsList>
            <TabsTrigger value="policies">Policy Rules</TabsTrigger>
            <TabsTrigger value="settings">Global Settings</TabsTrigger>
          </TabsList>

          <Select value={selectedCategory} onValueChange={setSelectedCategory}>
            <SelectTrigger className="w-40">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All Categories</SelectItem>
              <SelectItem value="access">Access Control</SelectItem>
              <SelectItem value="data">Data Protection</SelectItem>
              <SelectItem value="network">Network Security</SelectItem>
              <SelectItem value="crypto">Cryptographic</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <TabsContent value="policies" className="space-y-4 mt-4">
          {filteredPolicies.length === 0 ? (
            <Card>
              <CardContent className="p-8 text-center">
                <Shield className="h-12 w-12 mx-auto text-muted-foreground mb-4" />
                <p className="text-sm text-muted-foreground">
                  No policies found in the selected category.
                </p>
              </CardContent>
            </Card>
          ) : (
            filteredPolicies.map((policy) => (
              <Card key={policy.id}>
                <CardContent className="p-4">
                  <div className="flex items-start justify-between">
                    <div className="flex items-start space-x-3 flex-1">
                      <div className="mt-1">
                        {getCategoryIcon(policy.category)}
                      </div>
                      <div className="flex-1">
                        <div className="flex items-center space-x-2 mb-1">
                          <h3 className="font-medium">{policy.name}</h3>
                          <Badge className={getSeverityColor(policy.severity)}>
                            {getSeverityIcon(policy.severity)}
                            <span className="ml-1 capitalize">{policy.severity}</span>
                          </Badge>
                        </div>
                        <p className="text-sm text-muted-foreground mb-3">
                          {policy.description}
                        </p>
                        <div className="flex items-center space-x-4">
                          <div className="flex items-center space-x-2">
                            <Switch
                              id={policy.id}
                              checked={policy.enabled}
                              onCheckedChange={(enabled) => updatePolicy(policy.id, { enabled })}
                              disabled={isApplyingPolicy}
                            />
                            <Label htmlFor={policy.id} className="text-sm">
                              {policy.enabled ? 'Enabled' : 'Disabled'}
                            </Label>
                          </div>
                          <Badge variant="outline" className="capitalize">
                            {policy.category}
                          </Badge>
                        </div>
                      </div>
                    </div>
                  </div>
                </CardContent>
              </Card>
            ))
          )}
        </TabsContent>

        <TabsContent value="settings" className="space-y-4 mt-4">
          <Card>
            <CardHeader>
              <CardTitle>Global Security Settings</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label htmlFor="audit-retention">Audit Log Retention (days)</Label>
                  <Input id="audit-retention" type="number" defaultValue="90" />
                </div>
                <div>
                  <Label htmlFor="max-failed-attempts">Max Failed Login Attempts</Label>
                  <Input id="max-failed-attempts" type="number" defaultValue="5" />
                </div>
              </div>

              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <div>
                    <Label>Real-time Threat Detection</Label>
                    <p className="text-sm text-muted-foreground">Enable AI-powered anomaly detection</p>
                  </div>
                  <Switch defaultChecked />
                </div>

                <div className="flex items-center justify-between">
                  <div>
                    <Label>Automated Incident Response</Label>
                    <p className="text-sm text-muted-foreground">Automatically quarantine suspicious activities</p>
                  </div>
                  <Switch />
                </div>
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
