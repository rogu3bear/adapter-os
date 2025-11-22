"use client";

import * as React from "react";
import {
  useController,
  type FieldValues,
  type FieldPath,
  type UseControllerProps,
} from "react-hook-form";

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  SelectGroup,
  SelectLabel,
  SelectSeparator,
} from "../../ui/select";
import {
  FormItem,
  FormLabel,
  FormControl,
  FormDescription,
  FormMessage,
} from "../../ui/form";
import { cn } from "../../ui/utils";

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

export interface SelectOptionGroup {
  label: string;
  options: SelectOption[];
}

export type SelectFieldOption = SelectOption | SelectOptionGroup;

function isOptionGroup(option: SelectFieldOption): option is SelectOptionGroup {
  return "options" in option;
}

export interface SelectFieldProps<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
> extends Omit<UseControllerProps<TFieldValues, TName>, "defaultValue"> {
  /** Label displayed above the select */
  label?: string;
  /** Placeholder text when no value is selected */
  placeholder?: string;
  /** Description text displayed below the select */
  description?: string;
  /** Array of options or option groups */
  options: SelectFieldOption[];
  /** Additional CSS classes for the trigger */
  className?: string;
  /** Whether the field is disabled */
  disabled?: boolean;
  /** Size variant of the select trigger */
  size?: "sm" | "default";
  /** Whether the field is required */
  required?: boolean;
}

export function SelectField<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
>({
  name,
  control,
  rules,
  shouldUnregister,
  label,
  placeholder = "Select an option",
  description,
  options,
  className,
  disabled,
  size = "default",
  required,
}: SelectFieldProps<TFieldValues, TName>) {
  const {
    field,
    fieldState: { error },
  } = useController({
    name,
    control,
    rules,
    shouldUnregister,
  });

  return (
    <FormItem className="space-y-2">
      {label && (
        <FormLabel className={cn(error && "text-destructive")}>
          {label}
          {required && <span className="text-destructive ml-1">*</span>}
        </FormLabel>
      )}
      <FormControl>
        <Select
          value={field.value ?? ""}
          onValueChange={field.onChange}
          disabled={disabled}
        >
          <SelectTrigger
            ref={field.ref}
            className={cn(error && "border-destructive", className)}
            size={size}
            aria-invalid={!!error}
            onBlur={field.onBlur}
          >
            <SelectValue placeholder={placeholder} />
          </SelectTrigger>
          <SelectContent>
            {options.map((option, index) => {
              if (isOptionGroup(option)) {
                return (
                  <React.Fragment key={option.label}>
                    {index > 0 && <SelectSeparator />}
                    <SelectGroup>
                      <SelectLabel>{option.label}</SelectLabel>
                      {option.options.map((groupOption) => (
                        <SelectItem
                          key={groupOption.value}
                          value={groupOption.value}
                          disabled={groupOption.disabled}
                        >
                          {groupOption.label}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </React.Fragment>
                );
              }
              return (
                <SelectItem
                  key={option.value}
                  value={option.value}
                  disabled={option.disabled}
                >
                  {option.label}
                </SelectItem>
              );
            })}
          </SelectContent>
        </Select>
      </FormControl>
      {description && <FormDescription>{description}</FormDescription>}
      <FormMessage />
    </FormItem>
  );
}
