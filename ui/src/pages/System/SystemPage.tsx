import { useState } from 'react';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import SystemOverviewTab from './SystemOverviewTab';
import NodesTab from './NodesTab';
import WorkersTab from './WorkersTab';
import MemoryTab from './MemoryTab';
import MetricsTab from './MetricsTab';

export default function SystemPage() {
  const [activeTab, setActiveTab] = useState('overview');

  return (
    <DensityProvider pageKey="system">
      <FeatureLayout
        title="System"
        description="Monitor and manage system infrastructure"
        maxWidth="xl"
      >
        <Tabs value={activeTab} onValueChange={setActiveTab} className="w-full">
          <TabsList className="grid w-full grid-cols-5">
            <TabsTrigger value="overview">Overview</TabsTrigger>
            <TabsTrigger value="nodes">Nodes</TabsTrigger>
            <TabsTrigger value="workers">Workers</TabsTrigger>
            <TabsTrigger value="memory">Memory</TabsTrigger>
            <TabsTrigger value="metrics">Metrics</TabsTrigger>
          </TabsList>

          <TabsContent value="overview" className="mt-6">
            <SectionErrorBoundary sectionName="System Overview">
              <SystemOverviewTab />
            </SectionErrorBoundary>
          </TabsContent>

          <TabsContent value="nodes" className="mt-6">
            <SectionErrorBoundary sectionName="Nodes">
              <NodesTab />
            </SectionErrorBoundary>
          </TabsContent>

          <TabsContent value="workers" className="mt-6">
            <SectionErrorBoundary sectionName="Workers">
              <WorkersTab />
            </SectionErrorBoundary>
          </TabsContent>

          <TabsContent value="memory" className="mt-6">
            <SectionErrorBoundary sectionName="Memory">
              <MemoryTab />
            </SectionErrorBoundary>
          </TabsContent>

          <TabsContent value="metrics" className="mt-6">
            <SectionErrorBoundary sectionName="Metrics">
              <MetricsTab />
            </SectionErrorBoundary>
          </TabsContent>
        </Tabs>
      </FeatureLayout>
    </DensityProvider>
  );
}
