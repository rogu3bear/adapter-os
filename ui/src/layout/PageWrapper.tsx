/**
 * PageWrapper - Unified page wrapper component
 *
 * Combines DensityProvider + FeatureLayout + PageErrorsProvider into a single
 * wrapper to reduce repetitive nesting across 45+ pages.
 *
 * Before (old pattern):
 * ```tsx
 * <DensityProvider pageKey="my-page">
 *   <FeatureLayout title="Title" description="...">
 *     <PageErrorsProvider>
 *       <Content />
 *     </PageErrorsProvider>
 *   </FeatureLayout>
 * </DensityProvider>
 * ```
 *
 * After (with PageWrapper):
 * ```tsx
 * <PageWrapper pageKey="my-page" title="Title" description="...">
 *   <Content />
 * </PageWrapper>
 * ```
 */

import React from 'react';
import { DensityProvider } from '@/contexts/DensityContext';
import { PageErrorsProvider } from '@/components/ui/page-error-boundary';
import FeatureLayout from './FeatureLayout';
import type { PageHeaderAction, PageHeaderBadge } from '@/components/ui/page-header';
import type { InformationDensity } from '@/hooks/useInformationDensity';

type ContentPadding = 'default' | 'compact' | 'none';
type MaxWidth = 'md' | 'lg' | 'xl' | 'full';

interface PageWrapperProps {
  /** Unique key for persisting density preferences */
  pageKey: string;
  /** Page title displayed in the header */
  title: string;
  /** Optional page description */
  description?: string;
  /** Page content */
  children: React.ReactNode;

  // Density options
  /** Default density if not previously set by user */
  defaultDensity?: InformationDensity;
  /** Whether to persist density to localStorage (default: true) */
  persistDensity?: boolean;

  // FeatureLayout options
  /** Primary action button for the page header */
  primaryAction?: PageHeaderAction;
  /** Secondary action buttons for the page header */
  secondaryActions?: PageHeaderAction[];
  /** Badges to display in the page header */
  badges?: PageHeaderBadge[];
  /** Glossary term ID for the page header tooltip */
  termId?: string;
  /** Brief tooltip content for the page header */
  brief?: string;
  /** Adjust outer padding tokens (defaults to 'default' spacing) */
  contentPadding?: ContentPadding;
  /** Set the max width for the content area */
  maxWidth?: MaxWidth;

  // Optional split panel support
  /** Enable resizable split panes */
  resizable?: boolean;
  /** Storage key for persisting panel layout */
  storageKey?: string;
  /** Optional left panel content */
  left?: React.ReactNode;
  /** Optional right panel content */
  right?: React.ReactNode;
  /** Default panel layout percentages */
  defaultLayout?: number[];

  /**
   * Additional controls rendered to the right side of the header
   * @deprecated Use primaryAction and secondaryActions instead
   */
  headerActions?: React.ReactNode;
}

export function PageWrapper({
  pageKey,
  title,
  description,
  children,
  defaultDensity = 'comfortable',
  persistDensity = true,
  primaryAction,
  secondaryActions,
  badges,
  termId,
  brief,
  contentPadding = 'default',
  maxWidth = 'xl',
  resizable,
  storageKey,
  left,
  right,
  defaultLayout,
  headerActions,
}: PageWrapperProps) {
  return (
    <DensityProvider
      pageKey={pageKey}
      defaultDensity={defaultDensity}
      persist={persistDensity}
    >
      <FeatureLayout
        title={title}
        description={description}
        primaryAction={primaryAction}
        secondaryActions={secondaryActions}
        badges={badges}
        termId={termId}
        brief={brief}
        contentPadding={contentPadding}
        maxWidth={maxWidth}
        resizable={resizable}
        storageKey={storageKey}
        left={left}
        right={right}
        defaultLayout={defaultLayout}
        headerActions={headerActions}
      >
        <PageErrorsProvider>
          {children}
        </PageErrorsProvider>
      </FeatureLayout>
    </DensityProvider>
  );
}

export default PageWrapper;
