"use client";

import * as React from "react";
import * as TooltipPrimitive from '@radix-ui/react-tooltip';

import { cn, FROST_TOOLTIP } from "./utils";

/** Delay in ms before tooltip closes after hover ends. Default: 150ms */
const DEFAULT_CLOSE_DELAY_MS = 150;

function TooltipProvider({
  delayDuration = 0,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Provider>) {
  return (
    <TooltipPrimitive.Provider
      data-slot="tooltip-provider"
      delayDuration={delayDuration}
      {...props}
    />
  );
}

interface TooltipProps extends React.ComponentProps<typeof TooltipPrimitive.Root> {
  /** Delay in ms before tooltip closes after hover ends. Set to 0 for instant close. Default: 150 */
  closeDelayMs?: number;
  /** Delay before showing tooltip in ms. Default: 0 */
  delayDuration?: number;
}

function Tooltip({
  open: controlledOpen,
  defaultOpen = false,
  onOpenChange: consumerOnOpenChange,
  closeDelayMs = DEFAULT_CLOSE_DELAY_MS,
  delayDuration = 0,
  ...props
}: TooltipProps) {
  const [internalOpen, setInternalOpen] = React.useState(defaultOpen);
  const closeTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null);

  const isControlled = controlledOpen !== undefined;
  const open = isControlled ? controlledOpen : internalOpen;

  const clearCloseTimer = React.useCallback(() => {
    if (closeTimerRef.current) {
      clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
  }, []);

  const notifyOpenChange = React.useCallback(
    (nextOpen: boolean) => {
      consumerOnOpenChange?.(nextOpen);
      if (!isControlled) {
        setInternalOpen(nextOpen);
      }
    },
    [consumerOnOpenChange, isControlled],
  );

  const handleOpenChange = React.useCallback(
    (nextOpen: boolean) => {
      clearCloseTimer();

      if (nextOpen) {
        notifyOpenChange(true);
        return;
      }

      if (closeDelayMs > 0) {
        closeTimerRef.current = setTimeout(() => {
          notifyOpenChange(false);
        }, closeDelayMs);
      } else {
        notifyOpenChange(false);
      }
    },
    [clearCloseTimer, closeDelayMs, notifyOpenChange],
  );

  React.useEffect(() => {
    return () => clearCloseTimer();
  }, [clearCloseTimer]);

  return (
    <TooltipPrimitive.Root
      data-slot="tooltip"
      delayDuration={delayDuration}
      open={open}
      onOpenChange={handleOpenChange}
      {...props}
    />
  );
}

function TooltipTrigger({
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Trigger>) {
  return <TooltipPrimitive.Trigger data-slot="tooltip-trigger" {...props} />;
}

interface TooltipContentProps extends React.ComponentProps<typeof TooltipPrimitive.Content> {
  /** Hide the arrow pointer. Default: false */
  hideArrow?: boolean;
}

function TooltipContent({
  className,
  sideOffset = 4,
  children,
  hideArrow = false,
  ...props
}: TooltipContentProps) {
  // Normalize children to prevent duplicate rendering
  // Wrap string children in a fragment to ensure single rendering
  const content = React.useMemo(() => {
    if (typeof children === 'string') {
      return <>{children}</>;
    }
    return children;
  }, [children]);

  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Content
        data-slot="tooltip-content"
        sideOffset={sideOffset}
        className={cn(
          FROST_TOOLTIP,
          "animate-in fade-in-0 zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 z-50 w-fit max-w-md origin-(--radix-tooltip-content-transform-origin) rounded-md px-[var(--space-3)] py-[calc(var(--base-unit)*1.5)] text-xs text-balance",
          className,
        )}
        {...props}
      >
        {content}
        {!hideArrow && (
          <TooltipPrimitive.Arrow className="fill-popover/90 z-50 size-2.5 translate-y-[calc(-50%_-_var(--base-unit)*0.5)] rotate-45 rounded-[calc(var(--base-unit)*0.5)]" />
        )}
      </TooltipPrimitive.Content>
    </TooltipPrimitive.Portal>
  );
}

export { Tooltip, TooltipTrigger, TooltipContent, TooltipProvider };
