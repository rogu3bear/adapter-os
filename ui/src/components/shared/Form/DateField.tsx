"use client";

import * as React from "react";
import {
  useController,
  type FieldValues,
  type FieldPath,
  type UseControllerProps,
} from "react-hook-form";
import { CalendarIcon } from "lucide-react";

import { Calendar } from "../../ui/calendar";
import { Popover, PopoverContent, PopoverTrigger } from "../../ui/popover";
import { Button } from "../../ui/button";
import {
  FormItem,
  FormLabel,
  FormControl,
  FormDescription,
  FormMessage,
} from "../../ui/form";
import { cn } from "../../ui/utils";

/**
 * Format a date for display
 * Supports common format strings:
 * - "short": MM/DD/YYYY
 * - "medium": Month D, YYYY
 * - "long": Month D, YYYY (same as medium)
 * - "iso": YYYY-MM-DD
 * - Default: Month D, YYYY
 */
function formatDate(date: Date, formatStr: string = "medium"): string {
  const months = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December"
  ];
  const shortMonths = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"
  ];

  const day = date.getDate();
  const month = date.getMonth();
  const year = date.getFullYear();

  switch (formatStr) {
    case "short":
      return `${month + 1}/${day}/${year}`;
    case "iso":
      return `${year}-${String(month + 1).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
    case "medium":
    case "long":
    case "PPP":
    default:
      return `${months[month]} ${day}, ${year}`;
  }
}

export interface DateFieldProps<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
> extends Omit<UseControllerProps<TFieldValues, TName>, "defaultValue"> {
  /** Label displayed above the date picker */
  label?: string;
  /** Description text displayed below the date picker */
  description?: string;
  /** Placeholder text when no date is selected */
  placeholder?: string;
  /** Additional CSS classes for the trigger button */
  className?: string;
  /** Whether the field is disabled */
  disabled?: boolean;
  /** Whether the field is required */
  required?: boolean;
  /** Date format: "short" (MM/DD/YYYY), "medium" (Month D, YYYY), "iso" (YYYY-MM-DD), or "PPP" (Month D, YYYY) */
  dateFormat?: "short" | "medium" | "long" | "iso" | "PPP";
  /** Minimum selectable date */
  minDate?: Date;
  /** Maximum selectable date */
  maxDate?: Date;
  /** Dates that should be disabled */
  disabledDates?: Date[];
  /** Days of the week to disable (0 = Sunday, 6 = Saturday) */
  disabledDays?: number[];
}

export function DateField<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
>({
  name,
  control,
  rules,
  shouldUnregister,
  label,
  description,
  placeholder = "Select a date",
  className,
  disabled,
  required,
  dateFormat = "PPP",
  minDate,
  maxDate,
  disabledDates,
  disabledDays,
}: DateFieldProps<TFieldValues, TName>) {
  const [open, setOpen] = React.useState(false);

  const {
    field,
    fieldState: { error },
  } = useController({
    name,
    control,
    rules,
    shouldUnregister,
  });

  const selectedDate = field.value ? new Date(field.value as string | number | Date) : undefined;

  const handleSelect = (date: Date | undefined) => {
    field.onChange(date);
    setOpen(false);
  };

  const disabledMatcher = React.useMemo(() => {
    const matchers: Array<Date | ((date: Date) => boolean)> = [];

    if (minDate) {
      matchers.push((date: Date) => date < minDate);
    }

    if (maxDate) {
      matchers.push((date: Date) => date > maxDate);
    }

    if (disabledDates?.length) {
      disabledDates.forEach((d) => matchers.push(d));
    }

    if (disabledDays?.length) {
      matchers.push((date: Date) => disabledDays.includes(date.getDay()));
    }

    return matchers.length > 0 ? matchers : undefined;
  }, [minDate, maxDate, disabledDates, disabledDays]);

  return (
    <FormItem className="space-y-2">
      {label && (
        <FormLabel className={cn(error && "text-destructive")}>
          {label}
          {required && <span className="text-destructive ml-1">*</span>}
        </FormLabel>
      )}
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <FormControl>
            <Button
              ref={field.ref}
              variant="outline"
              role="combobox"
              aria-expanded={open}
              aria-invalid={!!error}
              disabled={disabled}
              onBlur={field.onBlur}
              className={cn(
                "w-full justify-start text-left font-normal",
                !selectedDate && "text-muted-foreground",
                error && "border-destructive",
                className
              )}
            >
              <CalendarIcon className="mr-2 size-4" />
              {selectedDate ? (
                formatDate(selectedDate, dateFormat)
              ) : (
                <span>{placeholder}</span>
              )}
            </Button>
          </FormControl>
        </PopoverTrigger>
        <PopoverContent className="w-auto p-0" align="start">
          <Calendar
            mode="single"
            selected={selectedDate}
            onSelect={handleSelect}
            disabled={disabledMatcher}
            initialFocus
          />
        </PopoverContent>
      </Popover>
      {description && <FormDescription>{description}</FormDescription>}
      <FormMessage />
    </FormItem>
  );
}
