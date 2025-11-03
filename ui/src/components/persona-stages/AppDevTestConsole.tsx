import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';

export default function AppDevTestConsole() {
  const title = 'AppDevTestConsole'.replace(/([A-Z])/g, ' $1').trim();
  return (
    <div className="flex items-center justify-center h-full">
      <Card className="w-full max-w-md">
        <CardHeader>
          <CardTitle className="text-center">{title}</CardTitle>
        </CardHeader>
        <CardContent className="text-center">
          <Badge variant="outline" className="mb-4">Mock Preview</Badge>
          <p className="text-sm text-muted-foreground">
            Interactive mock UI for this stage will be implemented in the full version.
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
