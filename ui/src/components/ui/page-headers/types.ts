import React from 'react';
import { LucideIcon } from 'lucide-react';

export interface BasePageHeaderAction {
  label: string;
  icon?: LucideIcon;
  onClick: () => void;
  variant?: 'default' | 'outline' | 'secondary' | 'destructive';
}

export interface CrudPageHeaderAction extends BasePageHeaderAction {
  // No additional fields for CRUD actions
}

export interface ConfigPageHeaderAction extends BasePageHeaderAction {
  loading?: boolean;
}

export interface BasePageHeaderProps {
  title: string;
  description?: string;
  secondaryActions?: React.ReactNode;
  className?: string;
}

export interface CrudPageHeaderProps extends BasePageHeaderProps {
  primaryAction?: CrudPageHeaderAction;
}

export interface ConfigPageHeaderProps extends BasePageHeaderProps {
  primaryAction?: ConfigPageHeaderAction;
}
