import { Code, Layers, GitBranch, Clock, Anchor, Flame, Thermometer, Square, Snowflake } from 'lucide-react';

export function getCategoryIcon(category: string) {
  switch (category) {
    case 'code':
      return <Code className="h-4 w-4 text-blue-500" />;
    case 'framework':
      return <Layers className="h-4 w-4 text-green-500" />;
    case 'codebase':
      return <GitBranch className="h-4 w-4 text-purple-500" />;
    case 'ephemeral':
      return <Clock className="h-4 w-4 text-orange-500" />;
    default:
      return <Code className="h-4 w-4" />;
  }
}

export function getStateBadgeVariant(state: string): 'default' | 'secondary' | 'outline' | 'destructive' {
  switch (state) {
    case 'resident':
      return 'default';
    case 'hot':
      return 'default';
    case 'warm':
      return 'secondary';
    case 'cold':
      return 'outline';
    case 'unloaded':
      return 'outline';
    default:
      return 'secondary';
  }
}

export function getStateIcon(state: string) {
  switch (state) {
    case 'resident':
      return <Anchor className="h-3 w-3" />;
    case 'hot':
      return <Flame className="h-3 w-3" />;
    case 'warm':
      return <Thermometer className="h-3 w-3" />;
    case 'cold':
      return <Snowflake className="h-3 w-3" />;
    case 'unloaded':
      return <Square className="h-3 w-3" />;
    default:
      return null;
  }
}
