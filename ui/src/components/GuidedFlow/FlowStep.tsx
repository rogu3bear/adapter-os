import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { CheckCircle, Circle, Loader2 } from 'lucide-react';
import { cn } from '@/components/ui/utils';

export type FlowStepStatus = 'pending' | 'active' | 'completed' | 'error';

interface FlowStepProps {
  stepNumber: number;
  title: string;
  description?: string;
  status: FlowStepStatus;
  children: React.ReactNode;
  className?: string;
}

export function FlowStep({
  stepNumber,
  title,
  description,
  status,
  children,
  className,
}: FlowStepProps) {
  const getStatusIcon = () => {
    switch (status) {
      case 'completed':
        return <CheckCircle className="h-5 w-5 text-green-500" />;
      case 'active':
        return <Loader2 className="h-5 w-5 text-blue-500 animate-spin" />;
      case 'error':
        return <Circle className="h-5 w-5 text-red-500" />;
      default:
        return <Circle className="h-5 w-5 text-muted-foreground" />;
    }
  };

  const getStatusBadge = () => {
    switch (status) {
      case 'completed':
        return <Badge variant="default" className="bg-green-500">Completed</Badge>;
      case 'active':
        return <Badge variant="default" className="bg-blue-500">In Progress</Badge>;
      case 'error':
        return <Badge variant="destructive">Error</Badge>;
      default:
        return <Badge variant="outline">Pending</Badge>;
    }
  };

  return (
    <Card
      className={cn(
        'transition-all',
        status === 'active' && 'ring-2 ring-primary',
        status === 'completed' && 'border-green-500',
        status === 'error' && 'border-red-500',
        className
      )}
    >
      <CardHeader>
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            {getStatusIcon()}
            <div>
              <CardTitle className="text-lg">
                Step {stepNumber}: {title}
              </CardTitle>
              {description && (
                <p className="text-sm text-muted-foreground mt-1">{description}</p>
              )}
            </div>
          </div>
          {getStatusBadge()}
        </div>
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  );
}

