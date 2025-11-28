import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { 
  Settings as SettingsIcon, 
  Users, 
  GitBranch,
  Server,
  Database,
  Shield,
  Key,
  Globe,
  FileText,
  Activity,
  AlertTriangle,
  CheckCircle,
  XCircle,
  Clock,
  Save,
  Trash2,
  Plus,
  Edit,
  Eye,
  Download,
  Upload
} from 'lucide-react';
import { Tenants } from './Tenants';
import { GitIntegrationPage } from './GitIntegrationPage';
import { Nodes } from './Nodes';
import apiClient from '../api/client';
import { User } from '../api/types';
import { toast } from 'sonner';

interface SettingsProps {
  user: User;
  selectedTenant: string;
}

export function Settings({ user, selectedTenant }: SettingsProps) {
  const [activeTab, setActiveTab] = useState('tenants');
  const [isLoading, setIsLoading] = useState(false);

  // Citation: docs/architecture/MasterPlan.md L8-L11
  const settingsTabs = [
    { id: 'tenants', label: 'Organizations', icon: Users, description: 'Multi-tenant management' },
    { id: 'nodes', label: 'Nodes', icon: Server, description: 'Compute infrastructure' },
    { id: 'git', label: 'Git Integration', icon: GitBranch, description: 'Repository management' },
    { id: 'system', label: 'System Config', icon: SettingsIcon, description: 'System configuration' }
  ];

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Settings</h1>
          <p className="text-muted-foreground">
            System configuration and administration
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Badge variant="outline" className="text-sm">
            Tenant: {selectedTenant}
          </Badge>
          <Badge variant="secondary" className="text-sm">
            {user.role}
          </Badge>
        </div>
      </div>

      {/* Settings Tabs */}
      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList className="grid w-full grid-cols-4">
          {settingsTabs.map((tab) => {
            const Icon = tab.icon;
            return (
              <TabsTrigger key={tab.id} value={tab.id} className="flex items-center gap-2">
                <Icon className="h-4 w-4" />
                <span className="hidden sm:inline">{tab.label}</span>
              </TabsTrigger>
            );
          })}
        </TabsList>

        {/* Tenants Tab */}
        <TabsContent value="tenants" className="space-y-4">
          <Tenants user={user} selectedTenant={selectedTenant} />
        </TabsContent>

        {/* Nodes Tab */}
        <TabsContent value="nodes" className="space-y-4">
          <Nodes user={user} selectedTenant={selectedTenant} />
        </TabsContent>

        {/* Git Integration Tab */}
        <TabsContent value="git" className="space-y-4">
          <GitIntegrationPage selectedTenant={selectedTenant} />
        </TabsContent>

        {/* System Config Tab */}
        <TabsContent value="system" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <SettingsIcon className="h-5 w-5" />
                System Configuration
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <Card>
                  <CardHeader>
                    <CardTitle className="text-lg">Database</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <div className="space-y-2">
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">Connection Status</span>
                        <Badge variant="outline" className="text-green-600">
                          <CheckCircle className="h-3 w-3 mr-1" />
                          Connected
                        </Badge>
                      </div>
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">Schema Version</span>
                        <span className="text-sm font-mono">v1.2.3</span>
                      </div>
                    </div>
                  </CardContent>
                </Card>

                <Card>
                  <CardHeader>
                    <CardTitle className="text-lg">Security</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <div className="space-y-2">
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">JWT Secret</span>
                        <Badge variant="outline" className="text-green-600">
                          <Shield className="h-3 w-3 mr-1" />
                          Configured
                        </Badge>
                      </div>
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">Policy Packs</span>
                        <span className="text-sm font-mono">22 Active</span>
                      </div>
                    </div>
                  </CardContent>
                </Card>
              </div>

              <div className="flex items-center gap-2 pt-4">
                <Button onClick={() => toast.info('System configuration saved')}>
                  <Save className="h-4 w-4 mr-2" />
                  Save Configuration
                </Button>
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}