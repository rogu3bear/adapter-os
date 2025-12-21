import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './ui/collapsible';
import { 
  Shield, 
  CheckCircle, 
  XCircle, 
  Lightbulb, 
  ChevronDown, 
  ChevronRight,
  User,
  Info
} from 'lucide-react';
import { UserRole } from '@/api/types';
import { getRoleGuidance } from '@/data/role-guidance';

interface RoleGuidanceProps {
  userRole: UserRole;
  className?: string;
}

export function RoleGuidance({ userRole, className }: RoleGuidanceProps) {
  const [isExpanded, setIsExpanded] = useState(false);
  const guidance = getRoleGuidance(userRole);

  if (!guidance) {
    return null;
  }

  return (
    <Card className={className}>
      <CardHeader className="pb-3">
        <CardTitle className="flex items-center gap-2 text-lg">
          <User className="h-5 w-5" />
          Role Guidance: {guidance.title}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <p className="text-sm text-muted-foreground">
          {guidance.description}
        </p>

        <Collapsible open={isExpanded} onOpenChange={setIsExpanded}>
          <CollapsibleTrigger asChild>
            <Button variant="ghost" className="w-full justify-between p-0 h-auto">
              <span className="flex items-center gap-2">
                <Info className="h-4 w-4" />
                View Role Details
              </span>
              {isExpanded ? (
                <ChevronDown className="h-4 w-4" />
              ) : (
                <ChevronRight className="h-4 w-4" />
              )}
            </Button>
          </CollapsibleTrigger>
          
          <CollapsibleContent className="space-y-4 pt-2">
            {/* Capabilities */}
            <div>
              <h4 className="text-sm font-medium flex items-center gap-2 mb-2">
                <CheckCircle className="h-4 w-4 text-green-600" />
                Capabilities
              </h4>
              <div className="space-y-1">
                {guidance.capabilities.map((capability, index) => (
                  <div key={index} className="flex items-start gap-2">
                    <div className="w-1.5 h-1.5 rounded-full bg-green-600 mt-2 flex-shrink-0" />
                    <span className="text-xs text-muted-foreground">{capability}</span>
                  </div>
                ))}
              </div>
            </div>

            {/* Restrictions */}
            {guidance.restrictions.length > 0 && (
              <div>
                <h4 className="text-sm font-medium flex items-center gap-2 mb-2">
                  <XCircle className="h-4 w-4 text-red-600" />
                  Restrictions
                </h4>
                <div className="space-y-1">
                  {guidance.restrictions.map((restriction, index) => (
                    <div key={index} className="flex items-start gap-2">
                      <div className="w-1.5 h-1.5 rounded-full bg-red-600 mt-2 flex-shrink-0" />
                      <span className="text-xs text-muted-foreground">{restriction}</span>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* Tips */}
            <div>
              <h4 className="text-sm font-medium flex items-center gap-2 mb-2">
                <Lightbulb className="h-4 w-4 text-yellow-600" />
                Tips
              </h4>
              <div className="space-y-1">
                {guidance.tips.map((tip, index) => (
                  <div key={index} className="flex items-start gap-2">
                    <div className="w-1.5 h-1.5 rounded-full bg-yellow-600 mt-2 flex-shrink-0" />
                    <span className="text-xs text-muted-foreground">{tip}</span>
                  </div>
                ))}
              </div>
            </div>
          </CollapsibleContent>
        </Collapsible>
      </CardContent>
    </Card>
  );
}
