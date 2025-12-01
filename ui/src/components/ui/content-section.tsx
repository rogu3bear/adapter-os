import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './card';
import { Separator } from './separator';
import { cn } from './utils';
import { getContentSectionClasses, getCardHierarchyClasses } from '@/utils/visual-hierarchy';

interface ContentSectionProps {
  title?: string;
  subtitle?: string;
  children: React.ReactNode;
  className?: string;
  level?: 'primary' | 'secondary' | 'tertiary' | 'quaternary';
  variant?: 'default' | 'compact' | 'detailed';
  showSeparator?: boolean;
  actions?: React.ReactNode;
}

export function ContentSection({
  title,
  subtitle,
  children,
  className,
  level = 'secondary',
  variant = 'default',
  showSeparator = false,
  actions
}: ContentSectionProps) {
  const sectionClasses = getContentSectionClasses(level);
  const cardClasses = getCardHierarchyClasses(variant);

  return (
    <div className={cn(sectionClasses.section, className)}>
      <Card>
        {(title || subtitle || actions) && (
          <CardHeader className={cardClasses.header}>
            <div className="flex items-start justify-between">
              <div className="space-y-1">
                {title && (
                  <CardTitle className={sectionClasses.title}>
                    {title}
                  </CardTitle>
                )}
                {subtitle && (
                  <p className={sectionClasses.subtitle}>
                    {subtitle}
                  </p>
                )}
              </div>
              {actions && (
                <div className="flex items-center gap-2">
                  {actions}
                </div>
              )}
            </div>
          </CardHeader>
        )}
        
        <CardContent className={cardClasses.content}>
          {children}
        </CardContent>
        
        {showSeparator && (
          <div className="px-6">
            <Separator />
          </div>
        )}
      </Card>
    </div>
  );
}

interface ContentGridProps {
  children: React.ReactNode;
  columns?: 1 | 2 | 3 | 4;
  className?: string;
  gap?: 'sm' | 'md' | 'lg';
}

export function ContentGrid({
  children,
  columns = 2,
  className,
  gap = 'md'
}: ContentGridProps) {
  const gridClasses = {
    sm: 'gap-3',
    md: 'gap-4',
    lg: 'gap-6'
  };

  const columnClasses = {
    1: 'grid-cols-1',
    2: 'grid-cols-1 md:grid-cols-2',
    3: 'grid-cols-1 md:grid-cols-2 lg:grid-cols-3',
    4: 'grid-cols-1 md:grid-cols-2 lg:grid-cols-4'
  };

  return (
    <div className={cn(
      'grid',
      columnClasses[columns],
      gridClasses[gap],
      className
    )}>
      {children}
    </div>
  );
}

interface ContentListProps {
  items: Array<{
    id: string;
    title: string;
    subtitle?: string;
    icon?: React.ReactNode;
    actions?: React.ReactNode;
  }>;
  level?: 'primary' | 'secondary' | 'tertiary';
  className?: string;
  onItemClick?: (item: { id: string; title: string; subtitle?: string; icon?: React.ReactNode; actions?: React.ReactNode }) => void;
}

export function ContentList({
  items,
  level = 'secondary',
  className,
  onItemClick
}: ContentListProps) {
  const listClasses = getCardHierarchyClasses('default');

  return (
    <div className={cn(listClasses.content, className)}>
      {items.map((item) => (
        <div
          key={item.id}
          className={cn(
            'flex items-center justify-between p-3 rounded-lg border border-border hover:bg-muted/50 transition-colors',
            onItemClick && 'cursor-pointer'
          )}
          onClick={() => onItemClick?.(item)}
        >
          <div className="flex items-center space-x-3">
            {item.icon && (
              <div className="flex-shrink-0">
                {item.icon}
              </div>
            )}
            <div className="flex-1 min-w-0">
              <h4 className="text-sm font-medium truncate">
                {item.title}
              </h4>
              {item.subtitle && (
                <p className="text-xs text-muted-foreground truncate">
                  {item.subtitle}
                </p>
              )}
            </div>
          </div>
          {item.actions && (
            <div className="flex items-center gap-2">
              {item.actions}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
