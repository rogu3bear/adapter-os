import React from 'react';
import { Switch } from './switch';
import { Label } from './label';
import { Settings, Eye, EyeOff } from 'lucide-react';
import { cn } from '@/lib/utils';

interface AdvancedToggleProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label?: string;
  description?: string;
  className?: string;
}

export function AdvancedToggle({
  checked,
  onCheckedChange,
  label = "Advanced Options",
  description = "Show advanced configuration options",
  className,
}: AdvancedToggleProps) {
  return (
    <div className={cn("flex items-center justify-between space-x-2", className)}>
      <div className="space-y-1">
        <Label htmlFor="advanced-toggle" className="flex items-center gap-2 text-sm font-medium">
          <Settings className="h-4 w-4" />
          {label}
        </Label>
        {description && (
          <p className="text-xs text-muted-foreground">{description}</p>
        )}
      </div>
      <div className="flex items-center gap-2">
        {checked ? (
          <Eye className="h-4 w-4 text-muted-foreground" />
        ) : (
          <EyeOff className="h-4 w-4 text-muted-foreground" />
        )}
        <Switch
          id="advanced-toggle"
          checked={checked}
          onCheckedChange={onCheckedChange}
          aria-label={label}
        />
      </div>
    </div>
  );
}
