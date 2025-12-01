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
  onOpenChange: controlledOnOpenChange,
  closeDelayMs = DEFAULT_CLOSE_DELAY_MS,
  delayDuration = 0,
  ...props
}: TooltipProps) {
  const [internalOpen, setInternalOpen] = React.useState(false);
  const closeTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null);

  const isControlled = controlledOpen !== undefined;
  const open = isControlled ? controlledOpen : internalOpen;

  const clearCloseTimer = React.useCallback(() => {
    if (closeTimerRef.current) {
      clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
  }, []);

  const handleOpenChange = React.useCallback(
    (nextOpen: boolean) => {
      clearCloseTimer();

      if (nextOpen) {
        // Opening: apply immediately
        if (isControlled) {
          controlledOnOpenChange?.(true);
        } else {
          setInternalOpen(true);
        }
      } else {
        // Closing: apply after delay (timeout after hover ends)
        if (closeDelayMs > 0) {
          closeTimerRef.current = setTimeout(() => {
            if (isControlled) {
              controlledOnOpenChange?.(false);
            } else {
              setInternalOpen(false);
            }
          }, closeDelayMs);
        } else {
          // Instant close
          if (isControlled) {
            controlledOnOpenChange?.(false);
          } else {
            setInternalOpen(false);
          }
        }
      }
    },
    [isControlled, controlledOnOpenChange, clearCloseTimer, closeDelayMs]
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
  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Content
        data-slot="tooltip-content"
        sideOffset={sideOffset}
        className={cn(
          FROST_TOOLTIP,
          "animate-in fade-in-0 zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 z-50 w-fit max-w-md origin-(--radix-tooltip-content-transform-origin) rounded-md px-3 py-1.5 text-xs text-balance",
          className,
        )}
        {...props}
      >
        {children}
        {!hideArrow && (
          <TooltipPrimitive.Arrow className="fill-popover/90 z-50 size-2.5 translate-y-[calc(-50%_-_2px)] rotate-45 rounded-[2px]" />
        )}
      </TooltipPrimitive.Content>
    </TooltipPrimitive.Portal>
  );
}

export { Tooltip, TooltipTrigger, TooltipContent, TooltipProvider };
