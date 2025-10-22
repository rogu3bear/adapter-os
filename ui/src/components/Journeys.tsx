import React, { useCallback, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion'; // Add Accordion
import { Input } from '@/components/ui/input'; // Add Input
import { Button } from '@/components/ui/button'; // Add Button for pagination
import Mermaid from 'react-mermaid'; // Add Mermaid
import apiClient from '@/api/client';
import { JourneyResponse } from '@/api/types';
import { logger } from '@/utils/logger';

interface JourneysProps {
  user: { email: string; roles?: string[] };
  selectedTenant: string;
}

export function Journeys({ user, selectedTenant }: JourneysProps) {
  const [activeTab, setActiveTab] = useState('adapter-lifecycle');
  const [journeyId, setJourneyId] = useState('example-id');
  const [idInput, setIdInput] = useState(journeyId); // Dynamic input
  const [page, setPage] = useState(0);
  const PAGE_SIZE = 20;

  // Update query on idInput change
  React.useEffect(() => {
    setJourneyId(idInput);
    setPage(0); // Reset page
  }, [idInput]);

  const { data: journeyData, isLoading, error } = useQuery({
    queryKey: ['journey', activeTab, journeyId, selectedTenant],
    queryFn: async () => {
      // Fetch journey data - placeholder implementation
      // TODO: Implement proper journey endpoint in apiClient
      const mockJourney: JourneyResponse = {
        journey_type: activeTab,
        id: journeyId,
        data: {},
        states: [],
        created_at: new Date().toISOString(),
      };
      return mockJourney;
    },
    enabled: !!selectedTenant && !!journeyId,
  });

  if (isLoading) return <div>Loading journey...</div>;
  if (error) return <div>Error loading journey: {(error as Error).message}</div>;

  const paginatedStates = journeyData ? journeyData.states.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE) : [];
  const totalPages = Math.ceil((journeyData?.states.length || 0) / PAGE_SIZE);
  const handleAccordionChange = useCallback((details: string[]) => {
    logger.debug('Journey accordion toggled', {
      component: 'Journeys',
      expanded: details,
      tenantId: selectedTenant,
    });
  }, [selectedTenant]);

  const renderStates = (states: JourneyResponse['states']) => {
    const defaultVal = states.length > 0 ? ['item-0'] : []; // Conditional for 0 states
    return (
      <Accordion
        type="multiple"
        defaultValue={defaultVal}
        onValueChange={handleAccordionChange}
        className="w-full"
      >
        {states.map((state, idx) => (
          <AccordionItem key={idx} value={`item-${idx}-${state.state}`}>
            <AccordionTrigger>
              <CardTitle className="text-sm">{state.state}</CardTitle>
              <CardDescription className="text-xs">
                {new Date(state.timestamp.toString()).toLocaleString()}
              </CardDescription>
            </AccordionTrigger>
            <AccordionContent>
              <CardContent>
                <pre className="text-xs">{JSON.stringify(state.details, null, 2)}</pre>
              </CardContent>
            </AccordionContent>
          </AccordionItem>
        ))}
      </Accordion>
    );
  };

  const generateMermaid = (states: JourneyResponse['states']) => {
    if (!states.length) return '';
    let diagram = 'sequenceDiagram\n    participant Adapter\n';
    states.forEach(s => {
      diagram += `    Adapter->>Adapter: ${s.state}\n`;
    });
    return diagram;
  };

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
          <CardContent className="space-y-4">
            {/* Dynamic ID Input */}
            <div className="flex gap-2">
              <Input
                value={idInput}
                onChange={(e) => setIdInput(e.target.value)}
                placeholder="Enter journey ID"
                aria-label="Enter journey ID"
                className="w-64"
              />
              <Button onClick={() => setJourneyId(idInput)}>Load</Button>
            </div>

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
              </TabsList>
              <TabsContent value={activeTab} className="space-y-4">
                {journeyData ? (
                  <>
                    <div>
                      <h3 className="text-lg font-semibold">{journeyData.journey_type} for {journeyData.id}</h3>
                      <p className="text-sm text-muted-foreground">Created: {journeyData.created_at.toLocaleString()}</p>
                    </div>
                    {/* Mermaid Diagram */}
                    <Card>
                      <CardHeader>
                        <CardTitle>Workflow Diagram</CardTitle>
                      </CardHeader>
                      <CardContent>
                        <Mermaid chart={generateMermaid(journeyData.states)} />
                      </CardContent>
                    </Card>
                    {renderStates(paginatedStates)}
                    {/* Pagination */}
                    {totalPages > 1 && (
                      <div className="flex justify-between">
                        <Button variant="outline" onClick={() => setPage(p => Math.max(0, p-1))} disabled={page === 0}>
                          Previous
                        </Button>
                        <span>Page {page + 1} of {totalPages}</span>
                        <Button variant="outline" onClick={() => setPage(p => Math.min(totalPages - 1, p+1))} disabled={page === totalPages - 1}>
                          Next
                        </Button>
                      </div>
                    )}
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
