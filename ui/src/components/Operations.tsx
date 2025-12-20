import React, { useState, useEffect, useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';


import { useBreadcrumb } from '@/contexts/BreadcrumbContext';
import { useProgressiveDisclosure } from '@/hooks/tutorial/useProgressiveDisclosure';
import { AdvancedToggle } from './ui/advanced-toggle';
import { GlossaryTooltip } from './ui/glossary-tooltip';
import { 
  Activity, 
  FileText, 
  ArrowUp, 
  Settings,
  Play,
  Pause,
  Square,
  RefreshCw,
  CheckCircle,
  XCircle,
  Clock,
  Zap,
  Target,
  BarChart3,
  Eye,
  Download,
  Upload,
  AlertTriangle,
  Server,
  Database,
  GitBranch,
  Brain,
  Workflow,
  Link,
  Bell,
  Shield
} from 'lucide-react';
import { Plans } from './Plans';
import { Promotion } from './Promotion';
import { Telemetry } from './Telemetry';
import { InferencePlayground } from './InferencePlayground';
import { AlertsPage } from '@/pages/Alerts/AlertsPage';
import { FederationStatus } from './FederationStatus';
import { apiClient } from '@/api/services';
import { User } from '@/api/types';
import { toast } from 'sonner';

interface OperationsProps {
  user: User;
  selectedTenant: string;
}

export function Operations({ user, selectedTenant }: OperationsProps) {
  const [activeTab, setActiveTab] = useState('plans');
  const [isLoading, setIsLoading] = useState(false);


  const { addBreadcrumb, clearBreadcrumbs } = useBreadcrumb();
  const { isVisible: showAdvanced, toggle: toggleAdvanced } = useProgressiveDisclosure({
    key: 'operations-advanced',
    defaultVisible: false,
    persist: true
  });

  // Citation: docs/architecture/MasterPlan.md L41-L44

  const operationTabs = useMemo(() => [
    { id: 'plans', label: 'Plans', icon: FileText, description: 'Execution plan compilation', advanced: false },
    { id: 'promotion', label: 'Promotion', icon: ArrowUp, description: 'Control plane promotion gates', advanced: true },
    { id: 'telemetry', label: 'Telemetry', icon: Activity, description: 'Event bundle management', advanced: true },
    { id: 'inference', label: 'Inference', icon: Zap, description: 'Interactive inference testing', advanced: false },
    { id: 'alerts', label: 'Alerts', icon: Bell, description: 'System alerts and monitoring', advanced: false },
    { id: 'federation', label: 'Federation', icon: Link, description: 'Cross-host federation status', advanced: true }
  ], []);

  // Filter tabs based on advanced visibility
  const visibleTabs = operationTabs.filter(tab => !tab.advanced || showAdvanced);


  // Breadcrumbs are now derived statelessly from URL - no manual management needed

  // Set breadcrumb when component mounts
  useEffect(() => {
    clearBreadcrumbs();
    addBreadcrumb({
      id: 'operations',
      label: 'Operations',
      icon: Activity
    });
  }, [addBreadcrumb, clearBreadcrumbs]);

  // Update breadcrumb when tab changes
  useEffect(() => {
    const currentTab = operationTabs.find(tab => tab.id === activeTab);
    if (currentTab) {
      addBreadcrumb({
        id: `operations-${activeTab}`,
        label: currentTab.label,
        icon: currentTab.icon
      });
    }
  }, [activeTab, addBreadcrumb, operationTabs]);

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Operations</h1>
          <p className="text-muted-foreground">
            Runtime management, plan execution, and system monitoring
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

      {/* Advanced Options Toggle */}
      <Card>
        <CardContent className="pt-6">
          <AdvancedToggle
            checked={showAdvanced}
            onCheckedChange={toggleAdvanced}
            label="Advanced Operations"
            description="Show advanced operations like promotion gates and telemetry management"
          />
        </CardContent>
      </Card>

      {/* Operations Tabs */}
      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList className={`grid w-full ${showAdvanced ? 'grid-cols-6' : 'grid-cols-3'}`}>
          {visibleTabs.map((tab) => {
            const Icon = tab.icon;
            return (
              <GlossaryTooltip key={tab.id} termId={tab.id}>
                <TabsTrigger value={tab.id} className="flex items-center gap-2">
                  <Icon className="h-4 w-4" />
                  <span className="hidden sm:inline">{tab.label}</span>
                </TabsTrigger>
              </GlossaryTooltip>
            );
          })}
        </TabsList>

        {/* Plans Tab */}
        <TabsContent value="plans" className="space-y-4">
          <Plans user={user} selectedTenant={selectedTenant} />
        </TabsContent>

        {/* Promotion Tab */}
        <TabsContent value="promotion" className="space-y-4">
          <Promotion user={user} selectedTenant={selectedTenant} />
        </TabsContent>

        {/* Telemetry Tab */}
        <TabsContent value="telemetry" className="space-y-4">
          <Telemetry user={user} selectedTenant={selectedTenant} />
        </TabsContent>

        {/* Inference Tab */}
        <TabsContent value="inference" className="space-y-4">
          <InferencePlayground selectedTenant={selectedTenant} />
        </TabsContent>

        {/* Alerts Tab */}
        <TabsContent value="alerts" className="space-y-4">
          <AlertsPage selectedTenant={selectedTenant} />
        </TabsContent>

        {/* Federation Tab */}
        <TabsContent value="federation" className="space-y-4">
          <FederationStatus />
        </TabsContent>
      </Tabs>
    </div>
  );
}
