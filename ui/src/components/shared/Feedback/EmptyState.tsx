"use client";

import * as React from "react";
import { LucideIcon, FileX, Search, Inbox, FolderOpen, Database, Users } from "lucide-react";
import { cn } from "@/components/ui/utils";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";

export interface EmptyStateAction {
  label: string;
  onClick: () => void;
  variant?: "default" | "outline" | "secondary" | "ghost";
  icon?: LucideIcon;
}

export interface EmptyStateProps {
  icon?: LucideIcon;
  title: string;
  description: string;
  action?: EmptyStateAction;
  secondaryAction?: EmptyStateAction;
  children?: React.ReactNode;
  variant?: "default" | "card" | "minimal";
  size?: "sm" | "md" | "lg";
  className?: string;
}

const sizeClasses = {
  sm: {
    icon: "h-8 w-8",
    iconContainer: "p-3",
    title: "text-base",
    description: "text-sm",
    padding: "py-6",
  },
  md: {
    icon: "h-12 w-12",
    iconContainer: "p-4",
    title: "text-lg",
    description: "text-sm",
    padding: "py-12",
  },
  lg: {
    icon: "h-16 w-16",
    iconContainer: "p-6",
    title: "text-xl",
    description: "text-base",
    padding: "py-16",
  },
};

export function EmptyState({
  icon: Icon = Inbox,
  title,
  description,
  action,
  secondaryAction,
  children,
  variant = "default",
  size = "md",
  className,
}: EmptyStateProps) {
  const sizes = sizeClasses[size];

  const content = (
    <div
      className={cn(
        "flex flex-col items-center justify-center text-center",
        sizes.padding,
        className
      )}
    >
      <div
        className={cn(
          "rounded-full bg-muted mb-4",
          sizes.iconContainer
        )}
      >
        <Icon className={cn("text-muted-foreground", sizes.icon)} />
      </div>
      <h3 className={cn("font-semibold text-foreground mb-2", sizes.title)}>
        {title}
      </h3>
      <p
        className={cn(
          "text-muted-foreground max-w-md mb-4",
          sizes.description
        )}
      >
        {description}
      </p>
      {(action || secondaryAction) && (
        <div className="flex flex-wrap items-center justify-center gap-3">
          {action && (
            <Button
              variant={action.variant || "default"}
              onClick={action.onClick}
            >
              {action.icon && <action.icon className="h-4 w-4 mr-2" />}
              {action.label}
            </Button>
          )}
          {secondaryAction && (
            <Button
              variant={secondaryAction.variant || "outline"}
              onClick={secondaryAction.onClick}
            >
              {secondaryAction.icon && (
                <secondaryAction.icon className="h-4 w-4 mr-2" />
              )}
              {secondaryAction.label}
            </Button>
          )}
        </div>
      )}
      {children}
    </div>
  );

  if (variant === "card") {
    return (
      <Card className="border-dashed">
        <CardContent className="p-0">{content}</CardContent>
      </Card>
    );
  }

  if (variant === "minimal") {
    return (
      <div className={cn("text-center py-4", className)}>
        <Icon className="h-6 w-6 text-muted-foreground mx-auto mb-2" />
        <p className="text-sm text-muted-foreground">{title}</p>
      </div>
    );
  }

  return content;
}

// Pre-configured empty state templates
export const emptyStateTemplates = {
  noResults: (searchTerm?: string, onClear?: () => void) => (
    <EmptyState
      icon={Search}
      title="No results found"
      description={
        searchTerm
          ? `No items match "${searchTerm}". Try adjusting your search or filters.`
          : "No items match your current filters."
      }
      action={
        onClear
          ? { label: "Clear Search", onClick: onClear, variant: "outline" }
          : undefined
      }
    />
  ),

  noData: (resourceName: string, onCreate?: () => void) => (
    <EmptyState
      icon={FolderOpen}
      title={`No ${resourceName} yet`}
      description={`Get started by creating your first ${resourceName.toLowerCase()}.`}
      action={
        onCreate
          ? { label: `Create ${resourceName}`, onClick: onCreate }
          : undefined
      }
    />
  ),

  noAdapters: (onCreate?: () => void) => (
    <EmptyState
      icon={Database}
      title="No adapters found"
      description="Adapters help customize AI responses. Create one to get started."
      action={
        onCreate
          ? { label: "Create Adapter", onClick: onCreate }
          : undefined
      }
    />
  ),

  noDatasets: (onUpload?: () => void) => (
    <EmptyState
      icon={FileX}
      title="No datasets available"
      description="Upload a dataset to begin training custom adapters."
      action={
        onUpload
          ? { label: "Upload Dataset", onClick: onUpload }
          : undefined
      }
    />
  ),

  noUsers: (onInvite?: () => void) => (
    <EmptyState
      icon={Users}
      title="No team members"
      description="Invite team members to collaborate on your projects."
      action={
        onInvite
          ? { label: "Invite Members", onClick: onInvite }
          : undefined
      }
    />
  ),

  emptyList: (itemName: string) => (
    <EmptyState
      icon={Inbox}
      title={`No ${itemName}`}
      description={`There are no ${itemName.toLowerCase()} to display.`}
      variant="minimal"
      size="sm"
    />
  ),

  comingSoon: (featureName: string) => (
    <EmptyState
      icon={Inbox}
      title="Coming Soon"
      description={`${featureName} is currently under development and will be available soon.`}
    />
  ),
};

export default EmptyState;
