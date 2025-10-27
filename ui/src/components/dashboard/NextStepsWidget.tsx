import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { ArrowRight, Zap, CheckCircle, Activity } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

interface NextStepAction {
  id: string;
  title: string;
  description: string;
  icon: any;
  route: string;
  priority: 'high' | 'medium' | 'low';
}

export function NextStepsWidget() {
  const navigate = useNavigate();

  // Mock next actions - in production, determine based on system state
  const actions: NextStepAction[] = [
    {
      id: 'test-adapter',
      title: 'Test Latest Adapter',
      description: 'Run validation tests on adapter_v1.2.3',
      icon: CheckCircle,
      route: '/testing',
      priority: 'high'
    },
    {
      id: 'review-metrics',
      title: 'Review Performance Metrics',
      description: 'Check inference latency and throughput',
      icon: Activity,
      route: '/monitoring',
      priority: 'medium'
    },
    {
      id: 'start-training',
      title: 'Start New Training Job',
      description: 'Train adapter for Python codebase',
      icon: Zap,
      route: '/training',
      priority: 'low'
    }
  ];

  const getPriorityColor = (priority: NextStepAction['priority']) => {
    switch (priority) {
      case 'high':
        return 'border-l-red-500';
      case 'medium':
        return 'border-l-yellow-500';
      default:
        return 'border-l-blue-500';
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Next Steps</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="space-y-3">
          {actions.map((action) => {
            const Icon = action.icon;
            return (
              <div
                key={action.id}
                className={`flex items-start gap-3 p-3 border-l-4 rounded-r-lg bg-muted/50 hover:bg-muted transition-colors ${getPriorityColor(action.priority)}`}
              >
                <Icon className="h-5 w-5 text-muted-foreground mt-0.5 flex-shrink-0" />
                <div className="flex-1 min-w-0">
                  <h4 className="font-medium text-sm mb-0.5">{action.title}</h4>
                  <p className="text-xs text-muted-foreground">{action.description}</p>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => navigate(action.route)}
                  className="flex-shrink-0"
                >
                  <ArrowRight className="h-4 w-4" />
                </Button>
              </div>
            );
          })}
        </div>
      </CardContent>
    </Card>
  );
}

