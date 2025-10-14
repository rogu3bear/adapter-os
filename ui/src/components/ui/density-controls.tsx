import React from 'react';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './select';
import { Label } from './label';
import { Card, CardContent, CardHeader, CardTitle } from './card';
import { 
  Maximize2, 
  Square, 
  Minimize2,
  Layout,
  Eye,
  EyeOff
} from 'lucide-react';
import { InformationDensity } from '../../hooks/useInformationDensity';
import { cn } from './utils';

interface DensityControlsProps {
  density: InformationDensity;
  onDensityChange: (density: InformationDensity) => void;
  className?: string;
  showLabel?: boolean;
}

export function DensityControls({ 
  density, 
  onDensityChange, 
  className,
  showLabel = true
}: DensityControlsProps) {
  const densityOptions = [
    {
      value: 'compact' as const,
      label: 'Compact',
      description: 'More information, less spacing',
      icon: Minimize2
    },
    {
      value: 'comfortable' as const,
      label: 'Comfortable',
      description: 'Balanced information and spacing',
      icon: Square
    },
    {
      value: 'spacious' as const,
      label: 'Spacious',
      description: 'Less information, more spacing',
      icon: Maximize2
    }
  ];

  const currentOption = densityOptions.find(option => option.value === density);
  const Icon = currentOption?.icon || Square;

  return (
    <div className={cn("space-y-2", className)}>
      {showLabel && (
        <Label htmlFor="density-select" className="flex items-center gap-2 text-sm font-medium">
          <Layout className="h-4 w-4" />
          Information Density
        </Label>
      )}
      
      <Select value={density} onValueChange={onDensityChange}>
        <SelectTrigger id="density-select" className="w-full">
          <div className="flex items-center gap-2">
            <Icon className="h-4 w-4" />
            <SelectValue placeholder="Select density" />
          </div>
        </SelectTrigger>
        <SelectContent>
          {densityOptions.map((option) => {
            const OptionIcon = option.icon;
            return (
              <SelectItem key={option.value} value={option.value}>
                <div className="flex items-center gap-2">
                  <OptionIcon className="h-4 w-4" />
                  <div>
                    <div className="font-medium">{option.label}</div>
                    <div className="text-xs text-muted-foreground">{option.description}</div>
                  </div>
                </div>
              </SelectItem>
            );
          })}
        </SelectContent>
      </Select>
    </div>
  );
}

interface DensityPreviewProps {
  density: InformationDensity;
  className?: string;
}

export function DensityPreview({ density, className }: DensityPreviewProps) {
  const getPreviewContent = () => {
    switch (density) {
      case 'compact':
        return {
          title: 'Compact Layout',
          subtitle: 'Dense information display',
          items: ['Item 1', 'Item 2', 'Item 3'],
          spacing: 'p-2 space-y-1'
        };
      case 'comfortable':
        return {
          title: 'Comfortable Layout',
          subtitle: 'Balanced information display',
          items: ['Item 1', 'Item 2', 'Item 3'],
          spacing: 'p-4 space-y-2'
        };
      case 'spacious':
        return {
          title: 'Spacious Layout',
          subtitle: 'Relaxed information display',
          items: ['Item 1', 'Item 2', 'Item 3'],
          spacing: 'p-6 space-y-3'
        };
      default:
        return {
          title: 'Comfortable Layout',
          subtitle: 'Balanced information display',
          items: ['Item 1', 'Item 2', 'Item 3'],
          spacing: 'p-4 space-y-2'
        };
    }
  };

  const preview = getPreviewContent();

  return (
    <Card className={cn("w-full", className)}>
      <CardContent className={preview.spacing}>
        <div className="space-y-2">
          <h4 className="font-medium text-sm">{preview.title}</h4>
          <p className="text-xs text-muted-foreground">{preview.subtitle}</p>
          <div className="space-y-1">
            {preview.items.map((item, index) => (
              <div key={index} className="text-xs bg-muted p-1 rounded">
                {item}
              </div>
            ))}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
