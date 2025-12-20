import { cn } from '@/lib/utils';

export interface VisualHierarchyConfig {
  level: 'primary' | 'secondary' | 'tertiary' | 'quaternary';
  emphasis?: 'high' | 'medium' | 'low';
  spacing?: 'tight' | 'normal' | 'loose';
}

export function getVisualHierarchyClasses(config: VisualHierarchyConfig) {
  const { level, emphasis = 'medium', spacing = 'normal' } = config;

  // Base classes for each level
  const levelClasses = {
    primary: {
      container: 'space-y-6',
      title: 'text-2xl font-bold tracking-tight',
      subtitle: 'text-lg font-semibold',
      body: 'text-base',
      caption: 'text-sm text-muted-foreground'
    },
    secondary: {
      container: 'space-y-4',
      title: 'text-xl font-semibold tracking-tight',
      subtitle: 'text-base font-medium',
      body: 'text-sm',
      caption: 'text-xs text-muted-foreground'
    },
    tertiary: {
      container: 'space-y-3',
      title: 'text-lg font-medium',
      subtitle: 'text-sm font-medium',
      body: 'text-sm',
      caption: 'text-xs text-muted-foreground'
    },
    quaternary: {
      container: 'space-y-2',
      title: 'text-base font-medium',
      subtitle: 'text-sm',
      body: 'text-xs',
      caption: 'text-xs text-muted-foreground'
    }
  };

  // Emphasis modifiers
  const emphasisClasses = {
    high: {
      title: 'text-foreground',
      subtitle: 'text-foreground',
      body: 'text-foreground',
      caption: 'text-muted-foreground'
    },
    medium: {
      title: 'text-foreground',
      subtitle: 'text-muted-foreground',
      body: 'text-muted-foreground',
      caption: 'text-muted-foreground'
    },
    low: {
      title: 'text-muted-foreground',
      subtitle: 'text-muted-foreground',
      body: 'text-muted-foreground',
      caption: 'text-muted-foreground'
    }
  };

  // Spacing modifiers
  const spacingClasses = {
    tight: 'space-y-2',
    normal: 'space-y-4',
    loose: 'space-y-6'
  };

  const baseClasses = levelClasses[level];
  const emphasisModifiers = emphasisClasses[emphasis];
  const spacingModifier = spacingClasses[spacing];

  return {
    container: cn(baseClasses.container, spacingModifier),
    title: cn(baseClasses.title, emphasisModifiers.title),
    subtitle: cn(baseClasses.subtitle, emphasisModifiers.subtitle),
    body: cn(baseClasses.body, emphasisModifiers.body),
    caption: cn(baseClasses.caption, emphasisModifiers.caption)
  };
}

export function getContentSectionClasses(level: VisualHierarchyConfig['level'] = 'secondary') {
  return {
    section: 'mb-6',
    header: 'mb-4',
    title: getVisualHierarchyClasses({ level, emphasis: 'high' }).title,
    subtitle: getVisualHierarchyClasses({ level, emphasis: 'medium' }).subtitle,
    content: getVisualHierarchyClasses({ level, emphasis: 'medium' }).container,
    footer: 'mt-4 pt-4 border-t border-border'
  };
}

export function getCardHierarchyClasses(variant: 'default' | 'compact' | 'detailed' = 'default') {
  const variants = {
    default: {
      container: 'p-4 space-y-4',
      header: 'space-y-2',
      title: 'text-lg font-semibold',
      subtitle: 'text-sm text-muted-foreground',
      content: 'space-y-3',
      footer: 'pt-4 border-t border-border'
    },
    compact: {
      container: 'p-3 space-y-3',
      header: 'space-y-1',
      title: 'text-base font-medium',
      subtitle: 'text-xs text-muted-foreground',
      content: 'space-y-2',
      footer: 'pt-3 border-t border-border'
    },
    detailed: {
      container: 'p-6 space-y-6',
      header: 'space-y-3',
      title: 'text-xl font-semibold',
      subtitle: 'text-base text-muted-foreground',
      content: 'space-y-4',
      footer: 'pt-6 border-t border-border'
    }
  };

  return variants[variant];
}

export function getListHierarchyClasses(level: 'primary' | 'secondary' | 'tertiary' = 'secondary') {
  const levels = {
    primary: {
      container: 'space-y-4',
      item: 'flex items-center space-x-3 p-3 rounded-lg border border-border',
      title: 'text-base font-medium',
      subtitle: 'text-sm text-muted-foreground',
      icon: 'h-5 w-5 text-muted-foreground'
    },
    secondary: {
      container: 'space-y-3',
      item: 'flex items-center space-x-2 p-2 rounded-md border border-border',
      title: 'text-sm font-medium',
      subtitle: 'text-xs text-muted-foreground',
      icon: 'h-4 w-4 text-muted-foreground'
    },
    tertiary: {
      container: 'space-y-2',
      item: 'flex items-center space-x-2 p-1 rounded-sm',
      title: 'text-xs font-medium',
      subtitle: 'text-xs text-muted-foreground',
      icon: 'h-3 w-3 text-muted-foreground'
    }
  };

  return levels[level];
}
