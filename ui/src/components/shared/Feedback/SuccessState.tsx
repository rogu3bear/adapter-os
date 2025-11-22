"use client";

import * as React from "react";
import { CheckCircle, ArrowRight, Sparkles, PartyPopper, ThumbsUp } from "lucide-react";
import { cn } from "@/components/ui/utils";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";

export interface SuccessAction {
  label: string;
  onClick: () => void;
  variant?: "default" | "outline" | "secondary" | "ghost";
  primary?: boolean;
}

export interface SuccessStateProps {
  title: string;
  description: string;
  actions?: SuccessAction[];
  variant?: "default" | "celebration" | "milestone" | "minimal";
  autoHide?: boolean;
  hideDelay?: number;
  onHide?: () => void;
  showConfetti?: boolean;
  className?: string;
}

const variantConfig = {
  default: {
    icon: CheckCircle,
    iconBg: "bg-green-100 dark:bg-green-900/30",
    iconColor: "text-green-600 dark:text-green-400",
    borderColor: "border-green-200 dark:border-green-800",
    bgColor: "bg-green-50 dark:bg-green-950/50",
  },
  celebration: {
    icon: PartyPopper,
    iconBg: "bg-yellow-100 dark:bg-yellow-900/30",
    iconColor: "text-yellow-600 dark:text-yellow-400",
    borderColor: "border-yellow-200 dark:border-yellow-800",
    bgColor: "bg-yellow-50 dark:bg-yellow-950/50",
  },
  milestone: {
    icon: Sparkles,
    iconBg: "bg-blue-100 dark:bg-blue-900/30",
    iconColor: "text-blue-600 dark:text-blue-400",
    borderColor: "border-blue-200 dark:border-blue-800",
    bgColor: "bg-blue-50 dark:bg-blue-950/50",
  },
  minimal: {
    icon: ThumbsUp,
    iconBg: "bg-muted",
    iconColor: "text-muted-foreground",
    borderColor: "border-border",
    bgColor: "bg-background",
  },
};

export function SuccessState({
  title,
  description,
  actions = [],
  variant = "default",
  autoHide = false,
  hideDelay = 5000,
  onHide,
  showConfetti = false,
  className,
}: SuccessStateProps) {
  const [visible, setVisible] = React.useState(true);
  const config = variantConfig[variant];
  const Icon = config.icon;

  React.useEffect(() => {
    if (autoHide) {
      const timer = setTimeout(() => {
        setVisible(false);
        onHide?.();
      }, hideDelay);
      return () => clearTimeout(timer);
    }
  }, [autoHide, hideDelay, onHide]);

  if (!visible) return null;

  // Simple confetti effect (CSS-based)
  const confettiStyle = showConfetti
    ? {
        position: "relative" as const,
        overflow: "hidden" as const,
      }
    : {};

  return (
    <Card
      className={cn(
        "border",
        config.borderColor,
        config.bgColor,
        className
      )}
      style={confettiStyle}
    >
      {showConfetti && (
        <div className="absolute inset-0 pointer-events-none overflow-hidden">
          {[...Array(20)].map((_, i) => (
            <div
              key={i}
              className="absolute w-2 h-2 rounded-full animate-confetti"
              style={{
                left: `${Math.random() * 100}%`,
                animationDelay: `${Math.random() * 2}s`,
                backgroundColor: ["#f59e0b", "#10b981", "#3b82f6", "#ef4444", "#8b5cf6"][
                  i % 5
                ],
              }}
            />
          ))}
        </div>
      )}
      <CardContent className="flex flex-col items-center justify-center py-8 px-6 text-center relative z-10">
        <div
          className={cn(
            "rounded-full p-4 mb-4",
            config.iconBg
          )}
        >
          <Icon className={cn("h-8 w-8", config.iconColor)} />
        </div>
        <h3 className="text-lg font-semibold text-foreground mb-2">{title}</h3>
        <p className="text-sm text-muted-foreground max-w-md mb-4">
          {description}
        </p>
        {actions.length > 0 && (
          <div className="flex flex-wrap items-center justify-center gap-3">
            {actions.map((action, index) => (
              <Button
                key={index}
                variant={action.variant || (action.primary ? "default" : "outline")}
                onClick={action.onClick}
              >
                {action.label}
                {!action.primary && <ArrowRight className="h-4 w-4 ml-2" />}
              </Button>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// Inline success message for simpler use cases
export interface SuccessMessageProps {
  message: string;
  onDismiss?: () => void;
  className?: string;
}

export function SuccessMessage({ message, onDismiss, className }: SuccessMessageProps) {
  return (
    <div
      className={cn(
        "flex items-center gap-2 rounded-md bg-green-50 dark:bg-green-950/50 border border-green-200 dark:border-green-800 px-3 py-2 text-sm text-green-800 dark:text-green-200",
        className
      )}
      role="status"
    >
      <CheckCircle className="h-4 w-4 text-green-600 dark:text-green-400 shrink-0" />
      <span className="flex-1">{message}</span>
      {onDismiss && (
        <button
          type="button"
          onClick={onDismiss}
          className="text-green-600 dark:text-green-400 hover:text-green-800 dark:hover:text-green-200 transition-colors"
          aria-label="Dismiss"
        >
          &times;
        </button>
      )}
    </div>
  );
}

// Pre-configured success state templates
export const successTemplates = {
  adapterCreated: (adapterName: string, onTest?: () => void, onView?: () => void) => (
    <SuccessState
      title="Adapter Created Successfully!"
      description={`Your new adapter "${adapterName}" is ready to use.`}
      variant="celebration"
      showConfetti
      actions={[
        ...(onTest ? [{ label: "Test It Now", onClick: onTest, primary: true }] : []),
        ...(onView ? [{ label: "View All Adapters", onClick: onView }] : []),
      ]}
    />
  ),

  trainingComplete: (adapterName: string, onTest?: () => void, onDownload?: () => void) => (
    <SuccessState
      title="Training Complete!"
      description={`Your adapter "${adapterName}" has been trained and is ready to use.`}
      variant="milestone"
      actions={[
        ...(onTest ? [{ label: "Test Adapter", onClick: onTest, primary: true }] : []),
        ...(onDownload ? [{ label: "Download .aos File", onClick: onDownload }] : []),
      ]}
    />
  ),

  uploadComplete: (fileName: string, onView?: () => void) => (
    <SuccessState
      title="Upload Complete"
      description={`"${fileName}" has been uploaded successfully.`}
      actions={onView ? [{ label: "View File", onClick: onView }] : []}
    />
  ),

  saved: (onContinue?: () => void) => (
    <SuccessState
      title="Changes Saved"
      description="Your changes have been saved successfully."
      variant="minimal"
      autoHide
      hideDelay={3000}
      actions={onContinue ? [{ label: "Continue", onClick: onContinue }] : []}
    />
  ),

  deleted: (itemName: string, onUndo?: () => void) => (
    <SuccessState
      title="Deleted Successfully"
      description={`${itemName} has been removed.`}
      variant="minimal"
      autoHide
      hideDelay={5000}
      actions={onUndo ? [{ label: "Undo", onClick: onUndo, variant: "outline" }] : []}
    />
  ),
};

export default SuccessState;
