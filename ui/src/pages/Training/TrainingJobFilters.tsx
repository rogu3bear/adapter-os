import { useMemo } from 'react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Calendar } from '@/components/ui/calendar';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Search,
  X,
  Calendar as CalendarIcon,
  Filter,
} from 'lucide-react';
import type { TrainingStatus } from '@/api/training-types';
import type { FilterValue } from '@/hooks/ui/useFilter';
import { cn } from '@/lib/utils';
import { format } from 'date-fns';

export type TrainingJobFilterKey = 'search' | 'status' | 'dateRange';

export interface TrainingJobFiltersProps {
  filters: Partial<Record<TrainingJobFilterKey, FilterValue>>;
  onFilterChange: (key: TrainingJobFilterKey, value: FilterValue) => void;
  onClearFilters: () => void;
  activeFilterCount: number;
}

const STATUS_OPTIONS: { value: TrainingStatus | 'all'; label: string }[] = [
  { value: 'all', label: 'All Statuses' },
  { value: 'pending', label: 'Pending' },
  { value: 'running', label: 'Running' },
  { value: 'completed', label: 'Completed' },
  { value: 'failed', label: 'Failed' },
  { value: 'cancelled', label: 'Cancelled' },
  { value: 'paused', label: 'Paused' },
];

export function TrainingJobFilters({
  filters,
  onFilterChange,
  onClearFilters,
  activeFilterCount,
}: TrainingJobFiltersProps) {
  const searchValue = (filters.search as string) || '';
  const statusValue = (filters.status as string) || 'all';
  const dateRange = filters.dateRange as { start: string; end: string } | null;

  const startDate = useMemo(() => {
    if (dateRange?.start) {
      return new Date(dateRange.start);
    }
    return undefined;
  }, [dateRange?.start]);

  const endDate = useMemo(() => {
    if (dateRange?.end) {
      return new Date(dateRange.end);
    }
    return undefined;
  }, [dateRange?.end]);

  const handleSearchChange = (value: string) => {
    onFilterChange('search', value || null);
  };

  const handleStatusChange = (value: string) => {
    onFilterChange('status', value === 'all' ? null : value);
  };

  const handleDateRangeChange = (type: 'start' | 'end', date: Date | undefined) => {
    const currentRange = dateRange || { start: '', end: '' };
    const newRange = {
      ...currentRange,
      [type]: date ? format(date, 'yyyy-MM-dd') : '',
    };

    // Clear the range if both are empty
    if (!newRange.start && !newRange.end) {
      onFilterChange('dateRange', null);
    } else {
      onFilterChange('dateRange', newRange);
    }
  };

  const formatDateDisplay = (date: Date | undefined, placeholder: string) => {
    if (!date) return placeholder;
    return format(date, 'MMM d, yyyy');
  };

  return (
    <div className="flex flex-wrap items-center gap-3 p-4 bg-muted/30 rounded-lg border">
      {/* Search Input */}
      <div className="relative flex-1 min-w-[200px] max-w-[300px]">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
        <Input
          placeholder="Search by adapter name or job ID..."
          value={searchValue}
          onChange={(e) => handleSearchChange(e.target.value)}
          className="pl-9 pr-8"
        />
        {searchValue && (
          <button
            onClick={() => handleSearchChange('')}
            className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
          >
            <X className="h-4 w-4" />
          </button>
        )}
      </div>

      {/* Status Filter */}
      <Select value={statusValue} onValueChange={handleStatusChange}>
        <SelectTrigger className="w-[160px]" data-cy="status-filter">
          <SelectValue placeholder="Status" />
        </SelectTrigger>
        <SelectContent>
          {STATUS_OPTIONS.map((option) => (
            <SelectItem key={option.value} value={option.value} data-cy={`status-${option.value}`}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      {/* Date Range Filter - Start Date */}
      <Popover>
        <PopoverTrigger asChild>
          <Button
            variant="outline"
            className={cn(
              'w-[140px] justify-start text-left font-normal',
              !startDate && 'text-muted-foreground'
            )}
          >
            <CalendarIcon className="mr-2 h-4 w-4" />
            {formatDateDisplay(startDate, 'From')}
          </Button>
        </PopoverTrigger>
        <PopoverContent className="w-auto p-0" align="start">
          <Calendar
            mode="single"
            selected={startDate}
            onSelect={(date) => handleDateRangeChange('start', date)}
            initialFocus
          />
          {startDate && (
            <div className="p-2 border-t">
              <Button
                variant="ghost"
                size="sm"
                className="w-full"
                onClick={() => handleDateRangeChange('start', undefined)}
              >
                Clear
              </Button>
            </div>
          )}
        </PopoverContent>
      </Popover>

      {/* Date Range Filter - End Date */}
      <Popover>
        <PopoverTrigger asChild>
          <Button
            variant="outline"
            className={cn(
              'w-[140px] justify-start text-left font-normal',
              !endDate && 'text-muted-foreground'
            )}
          >
            <CalendarIcon className="mr-2 h-4 w-4" />
            {formatDateDisplay(endDate, 'To')}
          </Button>
        </PopoverTrigger>
        <PopoverContent className="w-auto p-0" align="start">
          <Calendar
            mode="single"
            selected={endDate}
            onSelect={(date) => handleDateRangeChange('end', date)}
            initialFocus
          />
          {endDate && (
            <div className="p-2 border-t">
              <Button
                variant="ghost"
                size="sm"
                className="w-full"
                onClick={() => handleDateRangeChange('end', undefined)}
              >
                Clear
              </Button>
            </div>
          )}
        </PopoverContent>
      </Popover>

      {/* Active Filters Badge & Clear Button */}
      {activeFilterCount > 0 && (
        <div className="flex items-center gap-2 ml-auto">
          <Badge variant="secondary" className="gap-1">
            <Filter className="h-3 w-3" />
            {activeFilterCount} active
          </Badge>
          <Button
            variant="ghost"
            size="sm"
            onClick={onClearFilters}
            className="text-muted-foreground hover:text-foreground"
          >
            <X className="h-4 w-4 mr-1" />
            Clear all
          </Button>
        </div>
      )}
    </div>
  );
}
