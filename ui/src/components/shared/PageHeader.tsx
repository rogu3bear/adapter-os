import React from 'react';
import type { RouteCluster } from '@/config/routes';
import {
  PageHeader as BasePageHeader,
  type PageHeaderAction,
  type PageHeaderBadge,
  type PageHeaderBreadcrumb,
} from '@/components/ui/page-header';
import { cn } from '@/lib/utils';
import { ClusterBreadcrumb } from '@/components/shared/ClusterBreadcrumb';

export interface PageHeaderProps {
  cluster: RouteCluster;
  title: string;
  description?: string;
  termId?: string;
  brief?: string;
  primaryAction?: PageHeaderAction;
  secondaryActions?: PageHeaderAction[];
  badges?: PageHeaderBadge[];
  breadcrumbs?: PageHeaderBreadcrumb[];
  className?: string;
  children?: React.ReactNode;
}

export function PageHeader({
  cluster,
  title,
  description,
  termId,
  brief,
  primaryAction,
  secondaryActions,
  badges,
  breadcrumbs,
  className,
  children,
}: PageHeaderProps) {
  const prefixedTitle = cluster ? `${cluster}: ${title}` : title;

  return (
    <div className={cn('space-y-3', className)}>
      <ClusterBreadcrumb cluster={cluster} />
      <BasePageHeader
        title={prefixedTitle}
        description={description}
        termId={termId}
        brief={brief}
        primaryAction={primaryAction}
        secondaryActions={secondaryActions}
        badges={badges}
        breadcrumbs={breadcrumbs}
      >
        {children}
      </BasePageHeader>
    </div>
  );
}

export type { PageHeaderAction, PageHeaderBadge, PageHeaderBreadcrumb } from '@/components/ui/page-header';

