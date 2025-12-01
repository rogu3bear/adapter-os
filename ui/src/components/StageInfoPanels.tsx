import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Badge } from './ui/badge';
import { Eye, HelpCircle, MapPin } from 'lucide-react';
import { Stage } from '@/data/persona-journeys';

interface StageInfoPanelsProps {
  stage: Stage;
}

export function StageInfoPanels({ stage }: StageInfoPanelsProps) {
  return (
    <div className="h-full flex flex-col space-y-4 p-4 overflow-y-auto">
      {/* What Appears Panel */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center space-x-2 text-sm">
            <Eye className="h-4 w-4 text-blue-500" />
            <span>What Appears</span>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-foreground leading-relaxed">
            {stage.content.whatAppears}
          </p>
        </CardContent>
      </Card>

      {/* Why Panel */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center space-x-2 text-sm">
            <HelpCircle className="h-4 w-4 text-green-500" />
            <span>Why</span>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-foreground leading-relaxed">
            {stage.content.why}
          </p>
        </CardContent>
      </Card>

      {/* Context Panel */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center space-x-2 text-sm">
            <MapPin className="h-4 w-4 text-orange-500" />
            <span>Context</span>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-2">
            <p className="text-sm text-foreground leading-relaxed">
              {stage.content.context}
            </p>
            <Badge variant="outline" className="text-xs">
              {stage.id.replace(/-/g, ' ').replace(/\b\w/g, l => l.toUpperCase())}
            </Badge>
          </div>
        </CardContent>
      </Card>

      {/* Additional Info */}
      <Card className="bg-muted/30">
        <CardContent className="pt-4">
          <div className="text-xs text-muted-foreground space-y-1">
            <div className="flex justify-between">
              <span>Stage ID:</span>
              <code className="bg-background px-1 rounded text-xs">{stage.id}</code>
            </div>
            {stage.content.mockComponent && (
              <div className="flex justify-between">
                <span>Mock Component:</span>
                <code className="bg-background px-1 rounded text-xs">{stage.content.mockComponent}</code>
              </div>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
