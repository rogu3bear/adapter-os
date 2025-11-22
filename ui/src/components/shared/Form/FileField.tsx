"use client";

import * as React from "react";
import {
  useController,
  type FieldValues,
  type FieldPath,
  type UseControllerProps,
} from "react-hook-form";
import { Upload, X, FileIcon } from "lucide-react";

import { Button } from "../../ui/button";
import {
  FormItem,
  FormLabel,
  FormControl,
  FormDescription,
  FormMessage,
} from "../../ui/form";
import { cn } from "../../ui/utils";

export interface FileFieldProps<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
> extends Omit<UseControllerProps<TFieldValues, TName>, "defaultValue"> {
  /** Label displayed above the file input */
  label?: string;
  /** Description text displayed below the file input */
  description?: string;
  /** Additional CSS classes for the drop zone */
  className?: string;
  /** Whether the field is disabled */
  disabled?: boolean;
  /** Whether the field is required */
  required?: boolean;
  /** Accepted file types (e.g., ".pdf,.doc" or "image/*") */
  accept?: string;
  /** Allow multiple file selection */
  multiple?: boolean;
  /** Maximum file size in bytes */
  maxSize?: number;
  /** Maximum number of files (when multiple is true) */
  maxFiles?: number;
  /** Custom text for the drop zone */
  dropzoneText?: string;
  /** Custom text for the browse button */
  browseText?: string;
}

function formatFileSize(bytes: number): string {
  if (bytes === 0) return "0 Bytes";
  const k = 1024;
  const sizes = ["Bytes", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

export function FileField<
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
  required,
  accept,
  multiple = false,
  maxSize,
  maxFiles = 10,
  dropzoneText = "Drag and drop files here, or",
  browseText = "Browse",
}: FileFieldProps<TFieldValues, TName>) {
  const [isDragOver, setIsDragOver] = React.useState(false);
  const inputRef = React.useRef<HTMLInputElement>(null);

  const {
    field,
    fieldState: { error },
  } = useController({
    name,
    control,
    rules,
    shouldUnregister,
  });

  const files: File[] = React.useMemo(() => {
    const value = field.value as unknown;
    if (!value) return [];
    // Check if it's a FileList (has length and item method)
    if (typeof value === "object" && value !== null && "length" in value && "item" in value) {
      return Array.from(value as FileList);
    }
    if (Array.isArray(value)) return value as File[];
    // Check if it looks like a File (has name and size properties)
    if (typeof value === "object" && value !== null && "name" in value && "size" in value) {
      return [value as File];
    }
    return [];
  }, [field.value]);

  const validateFiles = (fileList: File[]): { valid: File[]; errors: string[] } => {
    const valid: File[] = [];
    const errors: string[] = [];

    for (const file of fileList) {
      if (maxSize && file.size > maxSize) {
        errors.push(`${file.name} exceeds maximum size of ${formatFileSize(maxSize)}`);
        continue;
      }
      valid.push(file);
    }

    if (multiple && valid.length > maxFiles) {
      errors.push(`Maximum ${maxFiles} files allowed`);
      return { valid: valid.slice(0, maxFiles), errors };
    }

    return { valid, errors };
  };

  const handleFiles = (newFiles: File[]) => {
    const { valid } = validateFiles(newFiles);

    if (multiple) {
      const existingNames = new Set(files.map((f) => f.name));
      const uniqueNew = valid.filter((f) => !existingNames.has(f.name));
      const combined = [...files, ...uniqueNew].slice(0, maxFiles);
      field.onChange(combined);
    } else {
      field.onChange(valid[0] ?? null);
    }
  };

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (!disabled) {
      setIsDragOver(true);
    }
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(false);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(false);

    if (disabled) return;

    const droppedFiles = Array.from(e.dataTransfer.files);
    handleFiles(droppedFiles);
  };

  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const selectedFiles = e.target.files ? Array.from(e.target.files) : [];
    handleFiles(selectedFiles);
    // Reset input so same file can be selected again
    if (inputRef.current) {
      inputRef.current.value = "";
    }
  };

  const handleRemove = (index: number) => {
    if (multiple) {
      const updated = files.filter((_, i) => i !== index);
      field.onChange(updated.length > 0 ? updated : null);
    } else {
      field.onChange(null);
    }
  };

  const handleBrowseClick = () => {
    inputRef.current?.click();
  };

  return (
    <FormItem className="space-y-2">
      {label && (
        <FormLabel className={cn(error && "text-destructive")}>
          {label}
          {required && <span className="text-destructive ml-1">*</span>}
        </FormLabel>
      )}
      <FormControl>
        <div
          ref={field.ref}
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
          onDrop={handleDrop}
          className={cn(
            "relative rounded-lg border-2 border-dashed transition-colors",
            isDragOver
              ? "border-primary bg-primary/5"
              : "border-input hover:border-muted-foreground/50",
            error && "border-destructive",
            disabled && "opacity-50 cursor-not-allowed",
            className
          )}
        >
          <input
            ref={inputRef}
            type="file"
            accept={accept}
            multiple={multiple}
            disabled={disabled}
            onChange={handleInputChange}
            onBlur={field.onBlur}
            className="sr-only"
            aria-invalid={!!error}
          />

          {files.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-8 px-4">
              <Upload className="size-10 text-muted-foreground mb-4" />
              <p className="text-sm text-muted-foreground text-center mb-2">
                {dropzoneText}
              </p>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={handleBrowseClick}
                disabled={disabled}
              >
                {browseText}
              </Button>
              {(accept || maxSize) && (
                <p className="text-xs text-muted-foreground mt-3 text-center">
                  {accept && <span>Accepted: {accept}</span>}
                  {accept && maxSize && <span className="mx-1">|</span>}
                  {maxSize && <span>Max size: {formatFileSize(maxSize)}</span>}
                </p>
              )}
            </div>
          ) : (
            <div className="p-4 space-y-2">
              {files.map((file, index) => (
                <div
                  key={`${file.name}-${index}`}
                  className="flex items-center gap-3 p-2 rounded-md bg-muted/50"
                >
                  <FileIcon className="size-8 text-muted-foreground shrink-0" />
                  <div className="flex-1 min-w-0">
                    <p className="text-sm font-medium truncate">{file.name}</p>
                    <p className="text-xs text-muted-foreground">
                      {formatFileSize(file.size)}
                    </p>
                  </div>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-xs"
                    onClick={() => handleRemove(index)}
                    disabled={disabled}
                    aria-label={`Remove ${file.name}`}
                  >
                    <X className="size-4" />
                  </Button>
                </div>
              ))}
              {multiple && files.length < maxFiles && (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={handleBrowseClick}
                  disabled={disabled}
                  className="w-full mt-2"
                >
                  <Upload className="size-4 mr-2" />
                  Add more files
                </Button>
              )}
            </div>
          )}
        </div>
      </FormControl>
      {description && <FormDescription>{description}</FormDescription>}
      <FormMessage />
    </FormItem>
  );
}
