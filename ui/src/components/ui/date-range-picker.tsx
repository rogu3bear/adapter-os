"use client";

import * as React from "react";
import { CalendarIcon } from "lucide-react";
import { Calendar } from "./calendar";
import { Popover, PopoverContent, PopoverTrigger } from "./popover";
import { Button } from "./button";
import { Label } from "./label";
import { cn } from "@/lib/utils";

export interface DateRange {
  from: Date;
  to: Date;
}

export interface DateRangePickerProps {
  /** Current date range */
  value?: DateRange;
  /** Callback when date range changes */
  onChange?: (range: DateRange | undefined) => void;
  /** Label for the picker */
  label?: string;
  /** Additional CSS classes */
  className?: string;
  /** Whether the picker is disabled */
  disabled?: boolean;
  /** Minimum selectable date */
  minDate?: Date;
  /** Maximum selectable date */
  maxDate?: Date;
}

function formatDateRange(range: DateRange | undefined): string {
  if (!range?.from) {
    return "Select date range";
  }

  const formatDate = (date: Date) => {
    return date.toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  };

  if (range.to) {
    return `${formatDate(range.from)} - ${formatDate(range.to)}`;
  }

  return formatDate(range.from);
}

export function DateRangePicker({
  value,
  onChange,
  label,
  className,
  disabled,
  minDate,
  maxDate,
}: DateRangePickerProps) {
  const [open, setOpen] = React.useState(false);
  const [localRange, setLocalRange] = React.useState<DateRange | undefined>(value);

  React.useEffect(() => {
    setLocalRange(value);
  }, [value]);

  const handleSelect = (range: { from?: Date; to?: Date } | undefined) => {
    if (!range?.from) {
      setLocalRange(undefined);
      onChange?.(undefined);
      return;
    }

    const newRange: DateRange = {
      from: range.from,
      to: range.to || range.from,
    };

    setLocalRange(newRange);

    // Only close and trigger onChange when both dates are selected
    if (range.to) {
      onChange?.(newRange);
      setOpen(false);
    }
  };

  const disabledMatcher = React.useMemo(() => {
    const matchers: Array<Date | ((date: Date) => boolean)> = [];

    if (minDate) {
      matchers.push((date: Date) => date < minDate);
    }

    if (maxDate) {
      matchers.push((date: Date) => date > maxDate);
    }

    return matchers.length > 0 ? matchers : undefined;
  }, [minDate, maxDate]);

  return (
    <div className={cn("grid gap-2", className)}>
      {label && <Label>{label}</Label>}
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <Button
            id="date-range"
            variant="outline"
            disabled={disabled}
            className={cn(
              "w-full justify-start text-left font-normal",
              !localRange && "text-muted-foreground"
            )}
          >
            <CalendarIcon className="mr-2 h-4 w-4" />
            {formatDateRange(localRange)}
          </Button>
        </PopoverTrigger>
        <PopoverContent className="w-auto p-0" align="start">
          <Calendar
            mode="range"
            defaultMonth={localRange?.from}
            selected={localRange ? { from: localRange.from, to: localRange.to } : undefined}
            onSelect={handleSelect}
            disabled={disabledMatcher}
            numberOfMonths={2}
            initialFocus
          />
        </PopoverContent>
      </Popover>
    </div>
  );
}
