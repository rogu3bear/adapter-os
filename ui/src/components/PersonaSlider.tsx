import React from 'react';
import { Card, CardContent } from './ui/card';
import { Badge } from './ui/badge';
import { cn } from './ui/utils';
import { Persona } from '../data/persona-journeys';

interface PersonaSliderProps {
  personas: Persona[];
  currentPersonaIndex: number;
  onPersonaChange: (personaIndex: number) => void;
}

export function PersonaSlider({ personas, currentPersonaIndex, onPersonaChange }: PersonaSliderProps) {
  return (
    <div className="p-4 bg-background border-t">
      <div className="flex items-center justify-center space-x-2 overflow-x-auto pb-2">
        {personas.map((persona, index) => {
          const isActive = index === currentPersonaIndex;
          const IconComponent = persona.icon;

          return (
            <Card
              key={persona.id}
              className={cn(
                "flex-shrink-0 cursor-pointer transition-all duration-200 hover:shadow-md",
                "w-48 h-24 border-2",
                isActive
                  ? "border-primary bg-primary/5 shadow-md scale-105"
                  : "border-border hover:border-primary/50"
              )}
              onClick={() => onPersonaChange(index)}
            >
              <CardContent className="flex items-center space-x-3 p-3 h-full">
                <div className={cn(
                  "p-2 rounded-lg transition-colors",
                  isActive ? "bg-primary text-primary-foreground" : "bg-muted"
                )}>
                  <IconComponent className="h-5 w-5" />
                </div>

                <div className="flex-1 min-w-0">
                  <div className="flex items-center space-x-2">
                    <h3 className={cn(
                      "font-medium text-sm truncate",
                      isActive ? "text-primary" : "text-foreground"
                    )}>
                      {persona.name}
                    </h3>
                    {isActive && (
                      <Badge variant="secondary" className="text-xs px-1.5 py-0.5">
                        Active
                      </Badge>
                    )}
                  </div>
                  <p className="text-xs text-muted-foreground truncate mt-0.5">
                    {persona.description}
                  </p>
                </div>
              </CardContent>
            </Card>
          );
        })}
      </div>

      {/* Navigation hint */}
      <div className="text-center mt-2">
        <p className="text-xs text-muted-foreground">
          Click any persona to explore their journey through the system
        </p>
      </div>
    </div>
  );
}
