"use client";

import * as React from "react";
import {
  Controller,
  type FieldPath,
  type FieldValues,
} from "react-hook-form";

import { cn } from "../../ui/utils";
import { Input } from "../../ui/input";
import { Label } from "../../ui/label";
import type { FormFieldProps, SelectOption } from "./types";

/**
 * FormField - A wrapper component for form inputs with label, description, and error handling.
 *
 * Integrates with react-hook-form for form state management and supports Zod schema validation.
 *
 * @example
 * ```tsx
 * const form = useForm<FormData>({
 *   resolver: zodResolver(schema),
 * });
 *
 * <FormField
 *   form={form}
 *   name="email"
 *   label="Email Address"
 *   type="email"
 *   required
 *   description="We'll never share your email."
 * />
 * ```
 *
 * @example Custom render
 * ```tsx
 * <FormField
 *   form={form}
 *   name="bio"
 *   label="Biography"
 *   render={({ field }) => (
 *     <CustomTextarea {...field} />
 *   )}
 * />
 * ```
 */
export function FormField<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
>({
  form,
  name,
  label,
  description,
  placeholder,
  required,
  disabled,
  className,
  render,
  type = "text",
  options,
  rows = 3,
  min,
  max,
  step,
  autoComplete,
}: FormFieldProps<TFieldValues, TName>) {
  const id = React.useId();
  const fieldId = `${id}-${name}`;
  const descriptionId = `${fieldId}-description`;
  const errorId = `${fieldId}-error`;

  return (
    <Controller
      control={form.control}
      name={name}
      render={({ field, fieldState, formState }) => {
        const hasError = !!fieldState.error;
        const errorMessage = fieldState.error?.message;

        // Build aria-describedby based on available elements
        const ariaDescribedBy = [
          description ? descriptionId : null,
          hasError ? errorId : null,
        ]
          .filter(Boolean)
          .join(" ") || undefined;

        // Default input rendering based on type
        const renderInput = () => {
          if (render) {
            return render({ field, fieldState, formState });
          }

          const baseProps = {
            id: fieldId,
            disabled: disabled || form.formState.isSubmitting,
            placeholder,
            "aria-describedby": ariaDescribedBy,
            "aria-invalid": hasError,
            "aria-required": required,
          };

          switch (type) {
            case "textarea":
              return (
                <textarea
                  {...field}
                  {...baseProps}
                  rows={rows}
                  className={cn(
                    "flex min-h-[80px] w-full rounded-md border border-input bg-input-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50",
                    hasError && "border-destructive focus-visible:ring-destructive",
                  )}
                />
              );

            case "select":
              return (
                <select
                  {...field}
                  {...baseProps}
                  className={cn(
                    "flex h-10 w-full rounded-md border border-input bg-input-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50",
                    hasError && "border-destructive focus-visible:ring-destructive",
                  )}
                >
                  {placeholder && (
                    <option value="" disabled>
                      {placeholder}
                    </option>
                  )}
                  {options?.map((option: SelectOption) => (
                    <option
                      key={option.value}
                      value={option.value}
                      disabled={option.disabled}
                    >
                      {option.label}
                    </option>
                  ))}
                </select>
              );

            case "checkbox":
              return (
                <div className="flex items-center gap-2">
                  <input
                    {...field}
                    {...baseProps}
                    type="checkbox"
                    checked={field.value}
                    onChange={(e) => field.onChange(e.target.checked)}
                    className={cn(
                      "h-4 w-4 rounded border border-input text-primary focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50",
                      hasError && "border-destructive",
                    )}
                  />
                  {label && (
                    <Label
                      htmlFor={fieldId}
                      className={cn(
                        "text-sm font-normal cursor-pointer",
                        hasError && "text-destructive",
                      )}
                    >
                      {label}
                      {required && <span className="text-destructive ml-1">*</span>}
                    </Label>
                  )}
                </div>
              );

            case "number":
              return (
                <Input
                  {...field}
                  {...baseProps}
                  type="number"
                  min={min}
                  max={max}
                  step={step}
                  onChange={(e) => {
                    const value = e.target.value;
                    field.onChange(value === "" ? "" : Number(value));
                  }}
                  className={cn(
                    hasError && "border-destructive focus-visible:ring-destructive",
                  )}
                />
              );

            default:
              return (
                <Input
                  {...field}
                  {...baseProps}
                  type={type}
                  autoComplete={autoComplete}
                  className={cn(
                    hasError && "border-destructive focus-visible:ring-destructive",
                  )}
                />
              );
          }
        };

        // Checkbox renders label inline, so skip the top label
        if (type === "checkbox") {
          return (
            <div className={cn("grid gap-2", className)}>
              {renderInput()}
              {description && (
                <p
                  id={descriptionId}
                  className="text-sm text-muted-foreground ml-6"
                >
                  {description}
                </p>
              )}
              {hasError && errorMessage && (
                <p
                  id={errorId}
                  className="text-sm text-destructive ml-6"
                  role="alert"
                >
                  {errorMessage}
                </p>
              )}
            </div>
          );
        }

        return (
          <div className={cn("grid gap-2", className)}>
            {label && (
              <Label
                htmlFor={fieldId}
                className={cn(hasError && "text-destructive")}
              >
                {label}
                {required && <span className="text-destructive ml-1">*</span>}
              </Label>
            )}
            {renderInput()}
            {description && (
              <p
                id={descriptionId}
                className="text-sm text-muted-foreground"
              >
                {description}
              </p>
            )}
            {hasError && errorMessage && (
              <p
                id={errorId}
                className="text-sm text-destructive"
                role="alert"
              >
                {errorMessage}
              </p>
            )}
          </div>
        );
      }}
    />
  );
}

export default FormField;
