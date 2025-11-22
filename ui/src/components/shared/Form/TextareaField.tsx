"use client";

import * as React from "react";
import {
  useController,
  type FieldValues,
  type FieldPath,
  type UseControllerProps,
} from "react-hook-form";

import { Textarea } from "../../ui/textarea";
import {
  FormItem,
  FormLabel,
  FormControl,
  FormDescription,
  FormMessage,
} from "../../ui/form";
import { cn } from "../../ui/utils";

export interface TextareaFieldProps<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
> extends Omit<UseControllerProps<TFieldValues, TName>, "defaultValue">,
    Omit<React.ComponentProps<"textarea">, "name" | "defaultValue"> {
  /** Label displayed above the textarea */
  label?: string;
  /** Description text displayed below the textarea */
  description?: string;
  /** Maximum character count (displays counter when set) */
  maxLength?: number;
  /** Show character count even without maxLength */
  showCharacterCount?: boolean;
  /** Whether the field is required */
  required?: boolean;
}

export function TextareaField<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
>({
  name,
  control,
  rules,
  shouldUnregister,
  label,
  description,
  maxLength,
  showCharacterCount = false,
  className,
  required,
  ...textareaProps
}: TextareaFieldProps<TFieldValues, TName>) {
  const {
    field,
    fieldState: { error },
  } = useController({
    name,
    control,
    rules: {
      ...rules,
      maxLength: maxLength
        ? {
            value: maxLength,
            message: `Maximum ${maxLength} characters allowed`,
          }
        : undefined,
    },
    shouldUnregister,
  });

  const currentLength = typeof field.value === "string" ? field.value.length : 0;
  const showCounter = showCharacterCount || maxLength !== undefined;
  const isOverLimit = maxLength !== undefined && currentLength > maxLength;

  return (
    <FormItem className="space-y-2">
      {label && (
        <div className="flex items-center justify-between">
          <FormLabel className={cn(error && "text-destructive")}>
            {label}
            {required && <span className="text-destructive ml-1">*</span>}
          </FormLabel>
        </div>
      )}
      <FormControl>
        <Textarea
          {...textareaProps}
          {...field}
          value={field.value ?? ""}
          className={cn(error && "border-destructive", className)}
          aria-invalid={!!error}
          maxLength={maxLength}
        />
      </FormControl>
      <div className="flex items-start justify-between gap-2">
        <div className="flex-1">
          {description && <FormDescription>{description}</FormDescription>}
          <FormMessage />
        </div>
        {showCounter && (
          <span
            className={cn(
              "text-xs tabular-nums shrink-0",
              isOverLimit ? "text-destructive" : "text-muted-foreground"
            )}
          >
            {currentLength}
            {maxLength !== undefined && `/${maxLength}`}
          </span>
        )}
      </div>
    </FormItem>
  );
}
