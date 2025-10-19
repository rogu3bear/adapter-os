import React, { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { api } from '@/api/client'; // Assume API client exists
import { JourneyResponse } from '@/api/types'; // Add to types.ts: export interface JourneyResponse { ... } from backend

interface JourneysProps {
  user: { email: string; roles: string[] };
  selectedTenant: string;
}

export function Journeys({ user, selectedTenant }: JourneysProps) {
  const [activeTab, setActiveTab] = useState('adapter-lifecycle');
  const [journeyId, setJourneyId] = useState('example-id'); // Default or from props

  const { data: journeyData, isLoading, error } = useQuery({
    queryKey: ['journey', activeTab, journeyId, selectedTenant],
    queryFn: async () => {
      const response = await api.get(`/v1/journeys/${activeTab}/${journeyId}`, {
        headers: { 'X-Tenant-ID': selectedTenant },
      });
      return response.data as JourneyResponse;
    },
    enabled: !!selectedTenant && !!journeyId,
  });

  if (isLoading) return <div>Loading journey...</div>;
  if (error) return <div>Error loading journey: {(error as Error).message}</div>;

  const renderStates = (states: JourneyResponse['states']) => (
    <div className="space-y-2">
      {states.map((state, idx) => (
        <Card key={idx}>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm">{state.state}</CardTitle>
            <CardDescription className="text-xs">
              {new Date(state.timestamp.toString()).toLocaleString()}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <pre className="text-xs">{JSON.stringify(state.details, null, 2)}</pre>
          </CardContent>
        </Card>
      ))}
    </div>
  );

  return (
    <TooltipProvider>
      <div className="space-y-6">
        <Card>
          <CardHeader>
            <CardTitle>User Journeys Dashboard</CardTitle>
            <CardDescription>
              Visualize and track operational workflows for {selectedTenant}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Tabs value={activeTab} onValueChange={setActiveTab} className="space-y-4">
              <TabsList>
                <TabsTrigger value="adapter-lifecycle">
                  <Tooltip>
                    <TooltipTrigger>Adapter Lifecycle</TooltipTrigger>
                    <TooltipContent>Track adapter states from unloaded to hot</TooltipContent>
                  </Tooltip>
                </TabsTrigger>
                <TabsTrigger value="promotion-pipeline">Promotion Pipeline</TabsTrigger>
                <TabsTrigger value="monitoring-flow">Monitoring Flow</TabsTrigger>
                {/* Add more tabs for other journeys */}
              </TabsList>
              <TabsContent value={activeTab} className="space-y-4">
                {journeyData ? (
                  <>
                    <div>
                      <h3 className="text-lg font-semibold">{journeyData.journey_type} for {journeyData.id}</h3>
                      <p className="text-sm text-muted-foreground">Created: {journeyData.created_at.toLocaleString()}</p>
                    </div>
                    {renderStates(journeyData.states)}
                  </>
                ) : (
                  <p>Select a journey ID to view details.</p>
                )}
              </TabsContent>
            </Tabs>
          </CardContent>
        </Card>
      </div>
    </TooltipProvider>
  );
}
