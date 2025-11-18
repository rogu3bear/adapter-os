"use client";


import { Toaster as Sonner } from 'sonner';

import { Toaster as Sonner, ToasterProps } from "sonner@2.0.3";
>
import { useTheme } from "@/layout/LayoutProvider";

const Toaster = ({ ...props }: ToasterProps) => {
  const { theme } = useTheme();
  return (
    <Sonner
      theme={theme as ToasterProps["theme"]}
      className="toaster group z-40"
      style={{
        "--normal-bg": "var(--popover)",
        "--normal-text": "var(--popover-foreground)",
        "--normal-border": "var(--border)",
      } as React.CSSProperties}
      {...props}
    />
  );
};

export { Toaster };
