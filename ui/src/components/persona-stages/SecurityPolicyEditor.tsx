import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import { Button } from '../ui/button';
import { Switch } from '../ui/switch';
import { Label } from '../ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import { Input } from '../ui/input';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import { Shield, Lock, Eye, AlertTriangle, CheckCircle, Settings, Save } from 'lucide-react';

interface PolicyRule {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  severity: 'low' | 'medium' | 'high' | 'critical';
  category: 'access' | 'data' | 'network' | 'crypto';
}

const mockPolicies: PolicyRule[] = [
  {
    id: 'egress-control',
    name: 'Network Egress Control',
    description: 'Block all network egress during inference operations',
    enabled: true,
    severity: 'critical',
    category: 'network'
  },
  {
    id: 'determinism-validation',
    name: 'Backend Determinism Validation',
    description: 'Validate Metal/OpenCL backend deterministic execution',
    enabled: true,
    severity: 'high',
    category: 'crypto'
  },
  {
    id: 'tenant-isolation',
    name: 'Tenant Data Isolation',
    description: 'Enforce strict data separation between tenants',
    enabled: true,
    severity: 'critical',
    category: 'access'
  },
  {
    id: 'audit-logging',
    name: 'Comprehensive Audit Logging',
    description: 'Log all policy decisions and security events',
    enabled: true,
    severity: 'medium',
    category: 'data'
  },
  {
    id: 'jwt-eddsa-only',
    name: 'EdDSA JWT Authentication',
    description: 'Require Ed25519-based JWT tokens only',
    enabled: true,
    severity: 'high',
    category: 'crypto'
  },
  {
    id: 'entropy-monitoring',
    name: 'Router Entropy Monitoring',
    description: 'Monitor router gate distribution entropy',
    enabled: false,
    severity: 'medium',
    category: 'crypto'
  }
];

export default function SecurityPolicyEditor() {
  const [policies, setPolicies] = useState(mockPolicies);
  const [selectedCategory, setSelectedCategory] = useState<string>('all');
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(false);

  const updatePolicy = (id: string, updates: Partial<PolicyRule>) => {
    setPolicies(prev => prev.map(policy =>
      policy.id === id ? { ...policy, ...updates } : policy
    ));
    setHasUnsavedChanges(true);
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
          <Button>
            <Save className="h-4 w-4 mr-2" />
            Save Policies
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
          {filteredPolicies.map((policy) => (
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
          ))}
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
