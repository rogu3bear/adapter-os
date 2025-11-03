import React, { useState } from 'react';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from './dialog';
import { Button } from './button';
import { Label } from './label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './select';
import { RadioGroup, RadioGroupItem } from './radio-group';
import { Download, Calendar } from 'lucide-react';
import { Input } from './input';

export type ExportFormat = 'json' | 'csv';
export type ExportScope = 'all' | 'selected' | 'filtered';

export interface ExportDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onExport: (options: ExportOptions) => void | Promise<void>;
  itemName?: string;
  hasSelected?: boolean;
  hasFilters?: boolean;
  defaultFormat?: ExportFormat;
  defaultScope?: ExportScope;
  showDateRange?: boolean;
  defaultStartDate?: string;
  defaultEndDate?: string;
}

export interface ExportOptions {
  format: ExportFormat;
  scope: ExportScope;
  startDate?: string;
  endDate?: string;
}

export function ExportDialog({
  open,
  onOpenChange,
  onExport,
  itemName = 'items',
  hasSelected = false,
  hasFilters = false,
  defaultFormat = 'json',
  defaultScope = 'all',
  showDateRange = false,
  defaultStartDate,
  defaultEndDate
}: ExportDialogProps) {
  const [format, setFormat] = useState<ExportFormat>(defaultFormat);
  const [scope, setScope] = useState<ExportScope>(defaultScope);
  const [startDate, setStartDate] = useState<string>(defaultStartDate || '');
  const [endDate, setEndDate] = useState<string>(defaultEndDate || '');

  const handleExport = async () => {
    const options: ExportOptions = {
      format,
      scope,
      ...(showDateRange && startDate && { startDate }),
      ...(showDateRange && endDate && { endDate })
    };
    await onExport(options);
  };

  const availableScopes: { value: ExportScope; label: string; disabled?: boolean }[] = [
    { value: 'all', label: `All ${itemName}` },
    { 
      value: 'selected', 
      label: `Selected ${itemName}`, 
      disabled: !hasSelected 
    },
    { 
      value: 'filtered', 
      label: `Filtered ${itemName}`, 
      disabled: !hasFilters 
    }
  ];

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle>Export {itemName}</DialogTitle>
        </DialogHeader>
        <div className="space-y-6 py-4">
          {/* Format Selection */}
          <div className="space-y-2">
            <Label>Format</Label>
            <Select value={format} onValueChange={(value) => setFormat(value as ExportFormat)}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="json">JSON</SelectItem>
                <SelectItem value="csv">CSV</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {/* Scope Selection */}
          <div className="space-y-2">
            <Label>Export Scope</Label>
            <RadioGroup value={scope} onValueChange={(value) => setScope(value as ExportScope)}>
              {availableScopes.map((option) => (
                <div key={option.value} className="flex items-center space-x-2">
                  <RadioGroupItem
                    value={option.value}
                    id={option.value}
                    disabled={option.disabled}
                  />
                  <Label
                    htmlFor={option.value}
                    className={`font-normal ${option.disabled ? 'text-muted-foreground cursor-not-allowed' : 'cursor-pointer'}`}
                  >
                    {option.label}
                  </Label>
                </div>
              ))}
            </RadioGroup>
          </div>

          {/* Date Range */}
          {showDateRange && (
            <div className="space-y-4">
              <Label className="flex items-center gap-2">
                <Calendar className="h-4 w-4" />
                Date Range
              </Label>
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label htmlFor="start-date" className="text-sm">Start Date</Label>
                  <Input
                    id="start-date"
                    type="date"
                    value={startDate}
                    onChange={(e) => setStartDate(e.target.value)}
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="end-date" className="text-sm">End Date</Label>
                  <Input
                    id="end-date"
                    type="date"
                    value={endDate}
                    onChange={(e) => setEndDate(e.target.value)}
                  />
                </div>
              </div>
            </div>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={handleExport}>
            <Download className="h-4 w-4 mr-2" />
            Export
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

