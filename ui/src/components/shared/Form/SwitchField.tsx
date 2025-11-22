"use client";

import * as React from "react";
import {
  useController,
  type FieldValues,
  type FieldPath,
  type UseControllerProps,
} from "react-hook-form";

import { Switch } from "../../ui/switch";
import { Label } from "../../ui/label";
import {
  FormItem,
  FormControl,
  FormDescription,
  FormMessage,
} from "../../ui/form";
import { cn } from "../../ui/utils";

export interface SwitchFieldProps<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
> extends Omit<UseControllerProps<TFieldValues, TName>, "defaultValue"> {
  /** Label displayed next to the switch */
  label?: string;
  /** Description text displayed below the switch */
  description?: string;
  /** Additional CSS classes for the container */
  className?: string;
  /** Whether the field is disabled */
  disabled?: boolean;
  /** Layout direction: row places label beside switch, column stacks them */
  layout?: "row" | "column";
  /** Position of the switch relative to the label (only for row layout) */
  switchPosition?: "left" | "right";
}

export function SwitchField<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
>({
  name,
  control,
  rules,
  shouldUnregister,
  label,
  description,
  className,
  disabled,
  layout = "row",
  switchPosition = "right",
}: SwitchFieldProps<TFieldValues, TName>) {
  const {
    field,
    fieldState: { error },
  } = useController({
    name,
    control,
    rules,
    shouldUnregister,
  });

  const id = React.useId();
  const switchId = `${id}-switch`;

  const switchElement = (
    <FormControl>
      <Switch
        id={switchId}
        ref={field.ref}
        checked={!!field.value}
        onCheckedChange={field.onChange}
        onBlur={field.onBlur}
        disabled={disabled}
        aria-invalid={!!error}
      />
    </FormControl>
  );

  if (layout === "column") {
    return (
      <FormItem className={cn("space-y-2", className)}>
        {label && (
          <Label
            htmlFor={switchId}
            className={cn(error && "text-destructive")}
          >
            {label}
          </Label>
        )}
        {switchElement}
        {description && <FormDescription>{description}</FormDescription>}
        <FormMessage />
      </FormItem>
    );
  }

  return (
    <FormItem
      className={cn(
        "flex items-center justify-between rounded-lg border p-4",
        error && "border-destructive",
        className
      )}
    >
      <div
        className={cn(
          "flex items-center gap-4",
          switchPosition === "left" && "flex-row-reverse justify-end flex-1"
        )}
      >
        {switchPosition === "left" && switchElement}
        <div className="space-y-0.5 flex-1">
          {label && (
            <Label
              htmlFor={switchId}
              className={cn(
                "text-base font-medium cursor-pointer",
                error && "text-destructive"
              )}
            >
              {label}
            </Label>
          )}
          {description && (
            <FormDescription className="text-sm">{description}</FormDescription>
          )}
          <FormMessage />
        </div>
        {switchPosition === "right" && switchElement}
      </div>
    </FormItem>
  );
}
