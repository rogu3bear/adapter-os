import React, { useState, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './card';
import { Input } from './input';
import { Label } from './label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './select';
import { Switch } from './switch';
import { Button } from './button';
import { Badge } from './badge';
import { X, Filter, ChevronDown, ChevronUp } from 'lucide-react';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './collapsible';
import { cn } from './utils';

export type FilterType = 'text' | 'select' | 'multiSelect' | 'dateRange' | 'toggle' | 'number';

export interface FilterOption {
  value: string;
  label: string;
}

export interface FilterConfig {
  id: string;
  label: string;
  type: FilterType;
  options?: FilterOption[];
  placeholder?: string;
  min?: number;
  max?: number;
}

export interface FilterValues {
  [filterId: string]: string | string[] | boolean | { start?: string; end?: string } | number | undefined;
}

interface AdvancedFilterProps {
  configs: FilterConfig[];
  values: FilterValues;
  onChange: (values: FilterValues) => void;
  onReset?: () => void;
  className?: string;
  defaultOpen?: boolean;
  title?: string;
}

export function AdvancedFilter({
  configs,
  values,
  onChange,
  onReset,
  className,
  defaultOpen = false,
  title = 'Filters',
}: AdvancedFilterProps) {
  const [isOpen, setIsOpen] = useState(defaultOpen);
  const [expandedGroups, setExpandedGroups] = useState<Record<string, boolean>>({});

  const updateFilter = useCallback((filterId: string, value: any) => {
    onChange({
      ...values,
      [filterId]: value,
    });
  }, [values, onChange]);

  const removeFilter = useCallback((filterId: string) => {
    const newValues = { ...values };
    delete newValues[filterId];
    onChange(newValues);
  }, [values, onChange]);

  const handleReset = useCallback(() => {
    if (onReset) {
      onReset();
    } else {
      onChange({});
    }
  }, [onReset, onChange]);

  const activeFilterCount = Object.keys(values).filter(key => {
    const value = values[key];
    if (value === undefined || value === null || value === '') return false;
    if (Array.isArray(value)) return value.length > 0;
    if (typeof value === 'object' && 'start' in value && 'end' in value) {
      return value.start || value.end;
    }
    return true;
  }).length;

  const renderFilter = (config: FilterConfig) => {
    const value = values[config.id];

    switch (config.type) {
      case 'text':
        return (
          <div key={config.id} className="space-y-2">
            <Label htmlFor={config.id}>{config.label}</Label>
            <Input
              id={config.id}
              type="text"
              placeholder={config.placeholder || `Search ${config.label.toLowerCase()}...`}
              value={(value as string) || ''}
              onChange={(e) => updateFilter(config.id, e.target.value || undefined)}
            />
          </div>
        );

      case 'select':
        return (
          <div key={config.id} className="space-y-2">
            <Label htmlFor={config.id}>{config.label}</Label>
            <Select
              value={(value as string) || 'all'}
              onValueChange={(val) => updateFilter(config.id, val === 'all' ? undefined : val)}
            >
              <SelectTrigger id={config.id}>
                <SelectValue placeholder={config.placeholder || `Select ${config.label.toLowerCase()}...`} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">All</SelectItem>
                {config.options?.filter(opt => opt.value !== '').map((opt) => (
                  <SelectItem key={opt.value} value={opt.value}>
                    {opt.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        );

      case 'multiSelect':
        const multiValues = Array.isArray(value) ? value : [];
        return (
          <div key={config.id} className="space-y-2">
            <Label>{config.label}</Label>
            <Select
              value=""
              onValueChange={(val) => {
                if (!multiValues.includes(val)) {
                  updateFilter(config.id, [...multiValues, val]);
                }
              }}
            >
              <SelectTrigger>
                <SelectValue placeholder={config.placeholder || `Add ${config.label.toLowerCase()}...`} />
              </SelectTrigger>
              <SelectContent>
                {config.options?.filter(opt => opt.value !== '' && !multiValues.includes(opt.value)).map((opt) => (
                  <SelectItem key={opt.value} value={opt.value}>
                    {opt.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {multiValues.length > 0 && (
              <div className="flex flex-wrap gap-2">
                {multiValues.map((val) => {
                  const option = config.options?.find(opt => opt.value === val);
                  return (
                    <Badge key={val} variant="secondary" className="gap-1">
                      {option?.label || val}
                      <button
                        onClick={() => {
                          updateFilter(config.id, multiValues.filter(v => v !== val));
                        }}
                        className="ml-1 hover:bg-destructive/20 rounded-full p-0.5"
                      >
                        <X className="h-3 w-3" />
                      </button>
                    </Badge>
                  );
                })}
              </div>
            )}
          </div>
        );

      case 'dateRange':
        const dateRange = typeof value === 'object' && value !== null && 'start' in value ? value as { start?: string; end?: string } : { start: undefined, end: undefined };
        return (
          <div key={config.id} className="space-y-2">
            <Label>{config.label}</Label>
            <div className="grid grid-cols-2 gap-2">
              <div className="space-y-1">
                <Label htmlFor={`${config.id}-start`} className="text-xs">Start</Label>
                <Input
                  id={`${config.id}-start`}
                  type="datetime-local"
                  value={dateRange.start || ''}
                  onChange={(e) => updateFilter(config.id, {
                    ...dateRange,
                    start: e.target.value || undefined,
                  })}
                />
              </div>
              <div className="space-y-1">
                <Label htmlFor={`${config.id}-end`} className="text-xs">End</Label>
                <Input
                  id={`${config.id}-end`}
                  type="datetime-local"
                  value={dateRange.end || ''}
                  onChange={(e) => updateFilter(config.id, {
                    ...dateRange,
                    end: e.target.value || undefined,
                  })}
                />
              </div>
            </div>
          </div>
        );

      case 'toggle':
        return (
          <div key={config.id} className="flex items-center justify-between space-x-2">
            <Label htmlFor={config.id}>{config.label}</Label>
            <Switch
              id={config.id}
              checked={value === true}
              onCheckedChange={(checked) => updateFilter(config.id, checked || undefined)}
            />
          </div>
        );

      case 'number':
        return (
          <div key={config.id} className="space-y-2">
            <Label htmlFor={config.id}>{config.label}</Label>
            <div className="grid grid-cols-2 gap-2">
              {config.min !== undefined && (
                <div className="space-y-1">
                  <Label htmlFor={`${config.id}-min`} className="text-xs">Min</Label>
                  <Input
                    id={`${config.id}-min`}
                    type="number"
                    placeholder={config.placeholder || 'Min'}
                    value={typeof value === 'object' && value !== null && 'min' in value ? (value as { min?: number }).min || '' : ''}
                    onChange={(e) => {
                      const current = typeof value === 'object' && value !== null && 'min' in value ? value as { min?: number; max?: number } : { min: undefined, max: undefined };
                      updateFilter(config.id, {
                        ...current,
                        min: e.target.value ? Number(e.target.value) : undefined,
                      });
                    }}
                  />
                </div>
              )}
              {config.max !== undefined && (
                <div className="space-y-1">
                  <Label htmlFor={`${config.id}-max`} className="text-xs">Max</Label>
                  <Input
                    id={`${config.id}-max`}
                    type="number"
                    placeholder={config.placeholder || 'Max'}
                    value={typeof value === 'object' && value !== null && 'max' in value ? (value as { max?: number }).max || '' : ''}
                    onChange={(e) => {
                      const current = typeof value === 'object' && value !== null && 'max' in value ? value as { min?: number; max?: number } : { min: undefined, max: undefined };
                      updateFilter(config.id, {
                        ...current,
                        max: e.target.value ? Number(e.target.value) : undefined,
                      });
                    }}
                  />
                </div>
              )}
              {config.min === undefined && config.max === undefined && (
                <Input
                  id={config.id}
                  type="number"
                  placeholder={config.placeholder || `Enter ${config.label.toLowerCase()}...`}
                  value={(value as number) || ''}
                  onChange={(e) => updateFilter(config.id, e.target.value ? Number(e.target.value) : undefined)}
                />
              )}
            </div>
          </div>
        );

      default:
        return null;
    }
  };

  return (
    <Card className={cn('w-full', className)}>
      <Collapsible open={isOpen} onOpenChange={setIsOpen}>
        <CollapsibleTrigger asChild>
          <CardHeader className="cursor-pointer hover:bg-muted/50 transition-colors">
            <div className="flex items-center justify-between">
              <CardTitle className="flex items-center gap-2">
                <Filter className="h-4 w-4" />
                {title}
                {activeFilterCount > 0 && (
                  <Badge variant="secondary" className="ml-2">
                    {activeFilterCount}
                  </Badge>
                )}
              </CardTitle>
              {isOpen ? <ChevronUp className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}
            </div>
          </CardHeader>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <CardContent className="space-y-4">
            {/* Active filter chips */}
            {activeFilterCount > 0 && (
              <div className="flex flex-wrap gap-2 pb-2 border-b">
                {Object.entries(values).map(([filterId, value]) => {
                  if (!value || (Array.isArray(value) && value.length === 0)) return null;
                  const config = configs.find(c => c.id === filterId);
                  if (!config) return null;

                  let label = '';
                  if (Array.isArray(value)) {
                    label = value.map(v => config.options?.find(opt => opt.value === v)?.label || v).join(', ');
                  } else if (typeof value === 'object' && 'start' in value) {
                    const range = value as { start?: string; end?: string };
                    label = `${range.start || '...'} - ${range.end || '...'}`;
                  } else if (typeof value === 'boolean') {
                    label = value ? 'Yes' : 'No';
                  } else {
                    label = String(value);
                  }

                  return (
                    <Badge key={filterId} variant="secondary" className="gap-1">
                      {config.label}: {label}
                      <button
                        onClick={() => removeFilter(filterId)}
                        className="ml-1 hover:bg-destructive/20 rounded-full p-0.5"
                      >
                        <X className="h-3 w-3" />
                      </button>
                    </Badge>
                  );
                })}
                <Button variant="ghost" size="sm" onClick={handleReset} className="h-6">
                  Clear all
                </Button>
              </div>
            )}

            {/* Filter inputs */}
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
              {configs.map(renderFilter)}
            </div>

            {activeFilterCount > 0 && (
              <div className="flex justify-end pt-2 border-t">
                <Button variant="outline" size="sm" onClick={handleReset}>
                  Reset Filters
                </Button>
              </div>
            )}
          </CardContent>
        </CollapsibleContent>
      </Collapsible>
    </Card>
  );
}

