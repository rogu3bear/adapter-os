import React, { useState, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { ChevronLeft, ChevronRight, Users } from 'lucide-react';
import { personas, getPersonaById } from '../data/persona-journeys';
import { PersonaSlider } from './PersonaSlider';
import { StageViewer } from './StageViewer';
import { StageInfoPanels } from './StageInfoPanels';

export function PersonaJourneyDemo() {
  // State for current persona and stage indices
  const [currentPersonaIndex, setCurrentPersonaIndex] = useState(0);
  const [currentStageIndex, setCurrentStageIndex] = useState(0);

  const currentPersona = personas[currentPersonaIndex];
  const currentStage = currentPersona.stages[currentStageIndex];

  // Navigation handlers
  const handlePersonaChange = useCallback((personaIndex: number) => {
    setCurrentPersonaIndex(personaIndex);
    setCurrentStageIndex(0); // Reset to first stage when switching personas
  }, []);

  const handlePreviousStage = useCallback(() => {
    if (currentStageIndex > 0) {
      setCurrentStageIndex(currentStageIndex - 1);
    } else if (currentPersonaIndex > 0) {
      // Go to previous persona's last stage
      const prevPersonaIndex = currentPersonaIndex - 1;
      const prevPersona = personas[prevPersonaIndex];
      setCurrentPersonaIndex(prevPersonaIndex);
      setCurrentStageIndex(prevPersona.stages.length - 1);
    }
  }, [currentPersonaIndex, currentStageIndex]);

  const handleNextStage = useCallback(() => {
    if (currentStageIndex < currentPersona.stages.length - 1) {
      setCurrentStageIndex(currentStageIndex + 1);
    } else if (currentPersonaIndex < personas.length - 1) {
      // Go to next persona's first stage
      setCurrentPersonaIndex(currentPersonaIndex + 1);
      setCurrentStageIndex(0);
    }
  }, [currentPersonaIndex, currentStageIndex, currentPersona.stages.length]);

  const canGoPrevious = currentPersonaIndex > 0 || currentStageIndex > 0;
  const canGoNext = currentPersonaIndex < personas.length - 1 || currentStageIndex < currentPersona.stages.length - 1;

  return (
    <div className="flex flex-col h-full bg-background">
      {/* Header */}
      <div className="flex items-center justify-between p-4 border-b">
        <div className="flex items-center space-x-2">
          <Users className="h-5 w-5" />
          <h1 className="text-xl font-semibold">Persona Journey Demo</h1>
        </div>
        <div className="text-sm text-muted-foreground">
          {currentPersona.name} • Stage {currentStageIndex + 1} of {currentPersona.stages.length}
        </div>
      </div>

      {/* Main Content Area */}
      <div className="flex-1 flex overflow-hidden">
        {/* Left Panel - Stage Viewer */}
        <div className="flex-1 flex flex-col">
          {/* Stage Navigation */}
          <div className="flex items-center justify-between p-4 border-b bg-muted/30">
            <Button
              variant="outline"
              size="sm"
              onClick={handlePreviousStage}
              disabled={!canGoPrevious}
            >
              <ChevronLeft className="h-4 w-4 mr-1" />
              Previous
            </Button>

            <div className="text-sm font-medium">
              {currentStage.title}
            </div>

            <Button
              variant="outline"
              size="sm"
              onClick={handleNextStage}
              disabled={!canGoNext}
            >
              Next
              <ChevronRight className="h-4 w-4 ml-1" />
            </Button>
          </div>

          {/* Stage Viewer */}
          <div className="flex-1 overflow-hidden">
            <StageViewer
              persona={currentPersona}
              stage={currentStage}
            />
          </div>
        </div>

        {/* Right Panel - Info Panels */}
        <div className="w-80 border-l bg-muted/20">
          <StageInfoPanels stage={currentStage} />
        </div>
      </div>

      {/* Bottom Persona Slider */}
      <div className="border-t bg-background">
        <PersonaSlider
          personas={personas}
          currentPersonaIndex={currentPersonaIndex}
          onPersonaChange={handlePersonaChange}
        />
      </div>
    </div>
  );
}
