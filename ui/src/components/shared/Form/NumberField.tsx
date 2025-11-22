"use client";

import * as React from "react";
import {
  useController,
  type FieldValues,
  type FieldPath,
  type UseControllerProps,
} from "react-hook-form";
import { Minus, Plus } from "lucide-react";

import { Input } from "../../ui/input";
import { Button } from "../../ui/button";
import {
  FormItem,
  FormLabel,
  FormControl,
  FormDescription,
  FormMessage,
} from "../../ui/form";
import { cn } from "../../ui/utils";

export interface NumberFieldProps<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
> extends Omit<UseControllerProps<TFieldValues, TName>, "defaultValue"> {
  /** Label displayed above the input */
  label?: string;
  /** Description text displayed below the input */
  description?: string;
  /** Placeholder text */
  placeholder?: string;
  /** Minimum allowed value */
  min?: number;
  /** Maximum allowed value */
  max?: number;
  /** Step increment for the input and buttons */
  step?: number;
  /** Show increment/decrement buttons */
  showButtons?: boolean;
  /** Additional CSS classes for the input */
  className?: string;
  /** Whether the field is disabled */
  disabled?: boolean;
  /** Whether the field is required */
  required?: boolean;
  /** Format displayed value (e.g., add units) */
  formatValue?: (value: number | undefined) => string;
  /** Parse input string to number */
  parseValue?: (value: string) => number | undefined;
  /** Prefix displayed before the input */
  prefix?: string;
  /** Suffix displayed after the input */
  suffix?: string;
}

export function NumberField<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
>({
  name,
  control,
  rules,
  shouldUnregister,
  label,
  description,
  placeholder,
  min,
  max,
  step = 1,
  showButtons = false,
  className,
  disabled,
  required,
  prefix,
  suffix,
}: NumberFieldProps<TFieldValues, TName>) {
  const {
    field,
    fieldState: { error },
  } = useController({
    name,
    control,
    rules: {
      ...rules,
      min:
        min !== undefined
          ? { value: min, message: `Minimum value is ${min}` }
          : undefined,
      max:
        max !== undefined
          ? { value: max, message: `Maximum value is ${max}` }
          : undefined,
    },
    shouldUnregister,
  });

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const rawValue = e.target.value;
    if (rawValue === "" || rawValue === "-") {
      field.onChange(rawValue === "" ? undefined : rawValue);
      return;
    }
    const parsed = parseFloat(rawValue);
    if (!isNaN(parsed)) {
      field.onChange(parsed);
    }
  };

  const handleIncrement = () => {
    const currentValue = typeof field.value === "number" ? field.value : 0;
    const newValue = currentValue + step;
    if (max === undefined || newValue <= max) {
      field.onChange(newValue);
    }
  };

  const handleDecrement = () => {
    const currentValue = typeof field.value === "number" ? field.value : 0;
    const newValue = currentValue - step;
    if (min === undefined || newValue >= min) {
      field.onChange(newValue);
    }
  };

  const displayValue =
    field.value === undefined || field.value === null ? "" : String(field.value);

  const canDecrement =
    !disabled &&
    (min === undefined ||
      (typeof field.value === "number" && field.value - step >= min));
  const canIncrement =
    !disabled &&
    (max === undefined ||
      (typeof field.value === "number" && field.value + step <= max));

  return (
    <FormItem className="space-y-2">
      {label && (
        <FormLabel className={cn(error && "text-destructive")}>
          {label}
          {required && <span className="text-destructive ml-1">*</span>}
        </FormLabel>
      )}
      <FormControl>
        <div className="flex items-center gap-2">
          {showButtons && (
            <Button
              type="button"
              variant="outline"
              size="icon-sm"
              onClick={handleDecrement}
              disabled={!canDecrement}
              aria-label="Decrease value"
            >
              <Minus className="size-4" />
            </Button>
          )}
          <div className="relative flex-1">
            {prefix && (
              <span className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground text-sm">
                {prefix}
              </span>
            )}
            <Input
              type="number"
              inputMode="decimal"
              {...field}
              value={displayValue}
              onChange={handleChange}
              min={min}
              max={max}
              step={step}
              placeholder={placeholder}
              disabled={disabled}
              aria-invalid={!!error}
              className={cn(
                error && "border-destructive",
                prefix && "pl-8",
                suffix && "pr-8",
                className
              )}
            />
            {suffix && (
              <span className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground text-sm">
                {suffix}
              </span>
            )}
          </div>
          {showButtons && (
            <Button
              type="button"
              variant="outline"
              size="icon-sm"
              onClick={handleIncrement}
              disabled={!canIncrement}
              aria-label="Increase value"
            >
              <Plus className="size-4" />
            </Button>
          )}
        </div>
      </FormControl>
      {description && <FormDescription>{description}</FormDescription>}
      <FormMessage />
    </FormItem>
  );
}
