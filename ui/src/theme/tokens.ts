import { logger } from '@/utils/logger';

export type ThemeMode = 'light' | 'dark' | 'system';

export type ThemePalette = {
  text: string;
  mutedText: string;
  background: string;
  surface: string;
  surfaceAlt: string;
  border: string;
  success: string;
  successSurface: string;
  warning: string;
  warningSurface: string;
  danger: string;
  dangerSurface: string;
};

export type ThemeTokens = {
  spacing: Record<'0' | '1' | '2' | '3' | '4' | '5' | '6', string>;
  radius: Record<'xs' | 's' | 'm' | 'l', string>;
  palette: ThemePalette;
};

const spacingScale: ThemeTokens['spacing'] = {
  0: '0px',
  1: '4px',
  2: '8px',
  3: '12px',
  4: '16px',
  5: '24px',
  6: '32px',
};

const radiusScale: ThemeTokens['radius'] = {
  xs: '4px',
  s: '8px',
  m: '12px',
  l: '16px',
};

const lightPalette: ThemePalette = {
  text: '#0f1115',
  mutedText: '#4b5563',
  background: '#ffffff',
  surface: '#f7f7f8',
  surfaceAlt: '#eef0f2',
  border: '#d1d5db',
  success: '#15803d',
  successSurface: '#ecfdf3',
  warning: '#b45309',
  warningSurface: '#fef3c7',
  danger: '#b91c1c',
  dangerSurface: '#fee2e2',
};

const darkPalette: ThemePalette = {
  text: '#f5f5f6',
  mutedText: '#9ca3af',
  background: '#0b0c10',
  surface: '#111827',
  surfaceAlt: '#1f2937',
  border: '#2d323b',
  success: '#34d399',
  successSurface: '#064e3b',
  warning: '#fbbf24',
  warningSurface: '#78350f',
  danger: '#f87171',
  dangerSurface: '#7f1d1d',
};

export const themeTokens: Record<'light' | 'dark', ThemeTokens> = {
  light: {
    spacing: spacingScale,
    radius: radiusScale,
    palette: lightPalette,
  },
  dark: {
    spacing: spacingScale,
    radius: radiusScale,
    palette: darkPalette,
  },
};

const setVar = (name: string, value: string) => {
  document.documentElement.style.setProperty(name, value);
};

export function resolveTheme(mode: ThemeMode): 'light' | 'dark' {
  if (mode === 'system') {
    if (typeof window !== 'undefined' && window.matchMedia) {
      return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
    }
    return 'light';
  }
  return mode;
}

export function applyTheme(mode: ThemeMode) {
  const resolved = resolveTheme(mode);
  const tokens = themeTokens[resolved];

  // Ensure dark class reflects resolved mode for tailwind/shadcn styles
  document.documentElement.classList.toggle('dark', resolved === 'dark');
  setVar('color-scheme', resolved);

  // Spacing tokens
  Object.entries(tokens.spacing).forEach(([key, value]) => {
    setVar(`--space-${key}`, value);
    setVar(`--spacing-${key}`, value);
  });
  setVar('--spacing', '4px');

  // Radius tokens
  Object.entries(tokens.radius).forEach(([key, value]) => {
    setVar(`--radius-${key}`, value);
  });
  setVar('--radius', tokens.radius.m);
  setVar('--radius-button', tokens.radius.s);
  setVar('--radius-card', tokens.radius.m);
  setVar('--radius-input', tokens.radius.s);
  setVar('--radius-surface', tokens.radius.l);

  const palette = tokens.palette;

  // Core surfaces and text
  setVar('--background', palette.background);
  setVar('--foreground', palette.text);
  setVar('--card', palette.surface);
  setVar('--card-foreground', palette.text);
  setVar('--popover', palette.surface);
  setVar('--popover-foreground', palette.text);
  setVar('--surface-1', palette.surface);
  setVar('--surface-2', palette.surfaceAlt);
  setVar('--surface-3', palette.surfaceAlt);
  setVar('--muted', palette.surfaceAlt);
  setVar('--muted-foreground', palette.mutedText);
  setVar('--border', palette.border);
  setVar('--input', palette.border);
  setVar('--input-background', palette.surfaceAlt);
  setVar('--ring', palette.text);

  // Primary/secondary accents keep grayscale emphasis
  setVar('--primary', palette.text);
  setVar('--primary-foreground', palette.background);
  setVar('--secondary', palette.surfaceAlt);
  setVar('--secondary-foreground', palette.text);
  setVar('--accent', palette.surfaceAlt);
  setVar('--accent-foreground', palette.text);

  // Status colors
  setVar('--success', palette.success);
  setVar('--success-surface', palette.successSurface);
  setVar('--success-border', palette.success);

  setVar('--warning', palette.warning);
  setVar('--warning-surface', palette.warningSurface);
  setVar('--warning-border', palette.warning);

  setVar('--destructive', palette.danger);
  setVar('--destructive-foreground', resolved === 'dark' ? palette.background : '#ffffff');
  setVar('--error', palette.danger);
  setVar('--error-surface', palette.dangerSurface);
  setVar('--error-border', palette.danger);

  setVar('--info', '#2563eb');
  setVar('--info-surface', resolved === 'dark' ? '#1e3a5f' : '#dbeafe');
  setVar('--info-border', resolved === 'dark' ? '#3b82f6' : '#60a5fa');

  // Sidebar tokens (fallback to surfaces)
  setVar('--sidebar', palette.surfaceAlt);
  setVar('--sidebar-foreground', palette.text);
  setVar('--sidebar-primary', palette.text);
  setVar('--sidebar-primary-foreground', palette.background);
  setVar('--sidebar-accent', palette.surface);
  setVar('--sidebar-accent-foreground', palette.text);
  setVar('--sidebar-border', palette.border);
  setVar('--sidebar-ring', palette.text);

  try {
    localStorage.setItem('theme', mode);
  } catch (error) {
    logger.warn('Failed to persist theme preference', { component: 'ThemeProvider' }, error as Error);
  }
}

