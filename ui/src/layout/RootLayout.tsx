import { useState, useEffect, useMemo, useCallback } from 'react';
import { Outlet, useLocation, useNavigate, Navigate } from 'react-router-dom';
import { Toaster } from '@/components/ui/sonner';
import { AppHeader } from '@/components/header';
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuButton,
  SidebarProvider,
  SidebarInset,
  SidebarTrigger,
  useSidebar,
} from '@/components/ui/sidebar';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';

import { TENANT_SELECTION_REQUIRED_KEY, useTheme, useAuth } from '@/providers/CoreProviders';
import { useTenant } from '@/providers/FeatureProviders';
import { CommandPaletteProvider, type CommandItem, useCommandPalette } from '@/contexts/CommandPaletteContext';
import { CopilotProvider } from '@/contexts/CopilotContext';
import { CommandPalette } from '@/components/CommandPalette';
import { CopilotDrawer } from '@/components/copilot/CopilotDrawer';
import { HelpCenter } from '@/components/HelpCenter';
import { NotificationCenter } from '@/components/NotificationCenter';
import { useKeyboardShortcuts } from '@/utils/accessibility';
import { generateNavigationGroups, shouldShowNavGroup } from '@/utils/navigation';
import { logger } from '@/utils/logger';
import { cn } from '@/lib/utils';
import { Lock, ChevronDown, ChevronRight, RefreshCw, LogOut, CheckCircle2, ArrowRight } from 'lucide-react';
import { LiveDataStatusProvider } from '@/hooks/realtime/useLiveDataStatus';
import { ConnectionStatusIndicator } from '@/components/header/ConnectionStatusIndicator';
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import LayoutDebugOverlay from '@/components/dev/LayoutDebugOverlay';
import { useLayoutDebug } from '@/hooks/ui/useLayoutDebug';
import { useSessionExpiryHandler } from '@/hooks/realtime/useSessionExpiryHandler';
import type { SessionMode } from '@/api/auth-types';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { TENANT_ACCESS_DENIED_EVENT } from '@/utils/tenant';
import { useUiMode } from '@/hooks/ui/useUiMode';
import { UiMode } from '@/config/ui-mode';
import { FetchErrorPanel } from '@/components/ui/fetch-error-panel';
import { useBackendReachability } from '@/stores/backendReachability';
import { isDemoMvpMode } from '@/config/demo';
import { KernelTelemetryProvider } from '@/contexts/KernelTelemetryContext';
import { KernelStatusBar } from '@/components/header/KernelStatusBar';
import { SystemBoot } from '@/components/system/SystemBoot';
import ScenarioController from '@/components/demo/ScenarioController';
import DemoWatermark from '@/components/demo/Watermark';
import { KernelTerminal } from '@/components/dev/KernelTerminal';

const COLLAPSED_GROUPS_KEY = 'aos_sidebar_collapsed_groups';

export function SessionModeBanner({ sessionMode }: { sessionMode: SessionMode }) {
  const isDemo = sessionMode === 'dev_bypass';
  const isDev = Boolean(import.meta.env.DEV);
  if (!isDemo && !isDev) return null;

  const importMeta = import.meta as {
    env?: {
      VITE_API_URL?: string;
      VITE_COMMIT_SHA?: string;
      VITE_BUILD_SHA?: string;
      VITE_GIT_SHA?: string;
    };
  };

  const apiBaseUrl = importMeta.env?.VITE_API_URL || '/api';
  const buildShaRaw =
    importMeta.env?.VITE_COMMIT_SHA ||
    importMeta.env?.VITE_BUILD_SHA ||
    importMeta.env?.VITE_GIT_SHA;
  const buildSha = buildShaRaw?.trim();
  const buildShaShort = buildSha ? buildSha.slice(0, 8) : null;
  const envLabel = isDemo ? 'Demo' : 'Dev';

  return (
    <Alert
      variant="warning"
      className="mb-[var(--space-3)] border-amber-200 bg-amber-50 text-amber-950"
      data-testid="env-banner"
    >
      <AlertTitle>{envLabel} environment</AlertTitle>
      <AlertDescription className="text-amber-950/80">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span>API:</span>
          <code className="text-xs font-mono text-amber-950">{apiBaseUrl}</code>
          {buildShaShort && (
            <>
              <span className="text-amber-950/40">•</span>
              <span>Commit:</span>
              <code className="text-xs font-mono text-amber-950" title={buildSha}>
                {buildShaShort}
              </code>
            </>
          )}
        </div>
        {isDemo && <div>Demo admin session. Do not use with real data.</div>}
      </AlertDescription>
    </Alert>
  );
}

interface RootLayoutContentProps {
  navigationGroups: ReturnType<typeof generateNavigationGroups>;
  tenantAccessDenied: boolean;
  clearTenantAccessDenied: () => void;
  uiMode: UiMode;
  onChangeUiMode: (mode: UiMode) => void;
  isKernelMode: boolean;
}

function RootLayoutContent({
  navigationGroups,
  tenantAccessDenied,
  clearTenantAccessDenied,
  uiMode,
  onChangeUiMode,
  isKernelMode,
}: RootLayoutContentProps) {
  const { theme, toggleTheme } = useTheme();
  const { user, logout, sessionMode } = useAuth();
  const backendReachability = useBackendReachability();
  const location = useLocation();
  const navigate = useNavigate();
  const { openPalette } = useCommandPalette();
  const { isMobile } = useSidebar();
  const { enabled: layoutDebugEnabled, toggle: toggleLayoutDebug } = useLayoutDebug();

  const [helpCenterOpen, setHelpCenterOpen] = useState(false);
  const [notificationCenterOpen, setNotificationCenterOpen] = useState(false);

  // Wire up keyboard shortcuts
  useKeyboardShortcuts({
    onSearch: openPalette,
    onHelp: () => setHelpCenterOpen(true),
    onNotifications: () => setNotificationCenterOpen(true),
  });

  useEffect(() => {
    const notificationsListener = () => setNotificationCenterOpen(true);
    const helpListener = () => setHelpCenterOpen(true);

    window.addEventListener('aos:open-notifications', notificationsListener);
    window.addEventListener('aos:open-help', helpListener);

    return () => {
      window.removeEventListener('aos:open-notifications', notificationsListener);
      window.removeEventListener('aos:open-help', helpListener);
    };
  }, []);

  useEffect(() => {
    const targetId = (location.hash ?? '').replace(/^#/, '').trim();
    if (!targetId || targetId.includes('=')) return;

    let cancelled = false;
    let attempts = 0;
    const maxAttempts = 40;
    const delayMs = 50;

    const tryScroll = () => {
      if (cancelled) return;

      const element = document.getElementById(targetId);
      if (element) {
        element.scrollIntoView({ block: 'start' });
        return;
      }

      attempts += 1;
      if (attempts >= maxAttempts) return;
      setTimeout(tryScroll, delayMs);
    };

    tryScroll();
    return () => {
      cancelled = true;
    };
  }, [location.hash, location.pathname]);

  // Persist collapsed groups state
  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>(() => {
    try {
      const saved = localStorage.getItem(COLLAPSED_GROUPS_KEY);
      return saved ? JSON.parse(saved) : {};
    } catch (e) {
      if (import.meta.env.DEV) {
        logger.warn('[RootLayout] Failed to load collapsed groups from localStorage', {
          component: 'RootLayout',
          operation: 'loadCollapsedGroups',
          error: e instanceof Error ? e.message : String(e),
        });
      }
      return {};
    }
  });

  const toggleGroup = (groupTitle: string) => {
    setCollapsedGroups(prev => {
      const next = { ...prev, [groupTitle]: !prev[groupTitle] };
      try {
        localStorage.setItem(COLLAPSED_GROUPS_KEY, JSON.stringify(next));
      } catch (e) {
        if (import.meta.env.DEV) {
          logger.warn('[RootLayout] Failed to save collapsed groups to localStorage', {
            component: 'RootLayout',
            operation: 'saveCollapsedGroups',
            groupTitle,
            error: e instanceof Error ? e.message : String(e),
          });
        }
      }
      return next;
    });
  };

  const navTestIdMap: Record<string, string> = {
    '/repos': 'nav-repos',
    '/training': 'nav-training',
    '/inference': 'nav-inference',
    '/security/policies': 'nav-policy',
    '/security/audit': 'nav-audit',
    '/security/evidence': 'nav-evidence',
  };

  return (
    <>
      {/* Skip Links for Accessibility */}
      <a
        href="#main-content"
        className="sr-only focus:not-sr-only focus:absolute focus:top-4 focus:left-4 focus:z-50 focus:px-4 focus:py-2 focus:bg-primary focus:text-primary-foreground focus:rounded-md focus:shadow-lg"
      >
        Skip to main content
      </a>
      <a
        href="#navigation"
        className="sr-only focus:not-sr-only focus:absolute focus:top-4 focus:left-48 focus:z-50 focus:px-4 focus:py-2 focus:bg-primary focus:text-primary-foreground focus:rounded-md focus:shadow-lg"
      >
        Skip to navigation
      </a>

      {/* Sidebar - collapsible to icon mode */}
      <Sidebar collapsible="icon">
        <SidebarContent id="navigation" className="pt-2" role="navigation" aria-label="Main navigation">
          {navigationGroups.filter(group => shouldShowNavGroup(group, user?.role ?? 'viewer')).map((group) => {
            const isCollapsed = collapsedGroups[group.title];
            const groupMenuId = `nav-group-${group.title.replace(/\s+/g, '-').toLowerCase()}`;
            return (
              <SidebarGroup key={group.title}>
                <SidebarGroupLabel asChild>
                  <button
                    onClick={() => toggleGroup(group.title)}
                    className={cn(
                      'flex items-center justify-between w-full cursor-pointer',
                      isMobile && 'min-h-[calc(var(--base-unit)*11)] px-[var(--space-3)] py-[var(--space-3)]'
                    )}
                    aria-expanded={!isCollapsed}
                    aria-label={`Toggle ${group.title} menu`}
                    aria-controls={groupMenuId}
                  >
                    <span>{group.title}</span>
                    {isCollapsed ? (
                      <ChevronRight className="h-3 w-3 flex-shrink-0" />
                    ) : (
                      <ChevronDown className="h-3 w-3 flex-shrink-0" />
                    )}
                  </button>
                </SidebarGroupLabel>

                {!isCollapsed && (
                  <SidebarMenu id={groupMenuId} aria-label={`${group.title} links`}>
                    {group.items.map((item) => {
                      const Icon = item.icon;
                      const isActive = location.pathname === item.to;
                      const navTestId = navTestIdMap[item.to];
                      return (
                        <SidebarMenuItem key={item.to}>
                          <SidebarMenuButton
                            isActive={isActive}
                            onClick={() => navigate(item.to)}
                            tooltip={item.label}
                            size={isMobile ? 'lg' : 'default'}
                            aria-label={`Navigate to ${item.label}`}
                            aria-current={isActive ? 'page' : undefined}
                            data-testid={navTestId}
                          >
                            <Icon className={isMobile ? 'h-5 w-5' : 'h-4 w-4'} />
                            <span>{item.label}</span>
                          </SidebarMenuButton>
                        </SidebarMenuItem>
                      );
                    })}
                  </SidebarMenu>
                )}
              </SidebarGroup>
            );
          })}
        </SidebarContent>
      </Sidebar>

      {/* Main content area — SidebarInset owns the primary vertical scroll. Inner panels should only scroll when truly overflowed. */}
      <SidebarInset>
        <SystemBoot />
        {/* Header */}
        <header
          className="border-b bg-background/80 backdrop-blur-sm sticky top-0 z-10"
          style={{
            paddingTop: 'env(safe-area-inset-top, 0px)',
            paddingLeft: 'env(safe-area-inset-left, 0px)',
            paddingRight: 'env(safe-area-inset-right, 0px)',
          }}
        >
          <div className="flex items-center">
            <AppHeader
              user={user ?? { email: 'guest@example.com', role: 'viewer' }}
              sessionMode={sessionMode}
              theme={theme}
              onLogout={() => void logout()}
              onOpenHelp={() => setHelpCenterOpen(true)}
              onOpenNotifications={setNotificationCenterOpen}
              onOpenPalette={openPalette}
              onToggleTheme={toggleTheme}
              className="flex-1 static top-auto z-auto border-0 bg-transparent"
              uiMode={uiMode}
              onChangeUiMode={onChangeUiMode}
            />
            {/* Global connection status indicator */}
            {isKernelMode && (
              <div className="px-2">
                <Button
                  variant={layoutDebugEnabled ? 'secondary' : 'ghost'}
                  size="sm"
                  onClick={toggleLayoutDebug}
                  disabled={!import.meta.env.DEV}
                  data-cy="layout-debug-toggle"
                >
                  {layoutDebugEnabled ? 'Hide Overlay' : 'Layout Overlay'}
                </Button>
              </div>
            )}
            <div className="px-3">
              <ConnectionStatusIndicator />
            </div>
          </div>
          {isKernelMode && <KernelStatusBar showEmergencyStop={isKernelMode} />}
        </header>

        {/* Content */}
        <main
          id="main-content"
          className="flex min-h-0 flex-1 flex-col"
          role="main"
          tabIndex={-1}
          style={{
            paddingLeft: 'max(var(--space-4), env(safe-area-inset-left, 0px))',
            paddingRight: 'max(var(--space-4), env(safe-area-inset-right, 0px))',
            paddingTop: 'var(--space-4)',
            paddingBottom: 'max(var(--space-4), env(safe-area-inset-bottom, 0px))',
          }}
        >
          <div
            className="mx-auto flex min-h-0 w-full flex-1 flex-col"
          style={{ maxWidth: 'var(--layout-content-width-xl)' }}
        >
          <div className="flex-none">
            <SessionModeBanner sessionMode={sessionMode} />
            <ScenarioController />
            {backendReachability.status === 'offline' && (
              <div className="mb-4">
                <FetchErrorPanel
                  title="Backend unavailable"
                  description="The UI can’t reach the AdapterOS API. Start the control plane and retry."
                    error={backendReachability.lastError?.error}
                  />
                </div>
              )}
              {tenantAccessDenied && (
                <Alert variant="destructive" className="mb-4 flex items-start gap-3">
                  <div className="flex-1">
                    <AlertTitle>TENANT_ACCESS_DENIED</AlertTitle>
                    <AlertDescription>
                      Your session lacks access to this tenant. Switch tenants and retry the action.
                    </AlertDescription>
                  </div>
                  <Button size="sm" variant="outline" onClick={clearTenantAccessDenied}>
                    Dismiss
                  </Button>
                </Alert>
              )}
            </div>

            <div className="min-h-0 flex-1">
              <SectionErrorBoundary sectionName="Page Content">
                <Outlet />
              </SectionErrorBoundary>
            </div>
          </div>
        </main>
      </SidebarInset>

      <CopilotDrawer />

      {/* Toaster stays above global overlays */}
      <Toaster position="top-right" className="z-[60]" />

      {/* Live region for screen reader announcements */}
      <div id="sr-announcer" aria-live="polite" aria-atomic="true" className="sr-only" />

      {/* Command Palette */}
      <CommandPalette />
      <NotificationCenter open={notificationCenterOpen} onOpenChange={setNotificationCenterOpen} />
      <HelpCenter open={helpCenterOpen} onOpenChange={setHelpCenterOpen} />
      {isKernelMode && <KernelTerminal visible={isKernelMode} />}
      <LayoutDebugOverlay enabled={layoutDebugEnabled} onToggle={toggleLayoutDebug} />
      <DemoWatermark />
    </>
  );
}

export default function RootLayout() {
  const { user, isLoading, logout, sessionMode } = useAuth();
  const { selectedTenant, tenants, setSelectedTenant, isLoading: tenantsLoading, refreshTenants } = useTenant();
  const location = useLocation();
  const [tenantError, setTenantError] = useState<string | null>(null);
  const [isSwitchingTenant, setIsSwitchingTenant] = useState(false);
  const [tenantAccessDenied, setTenantAccessDenied] = useState(false);
  const { uiMode, setUiMode } = useUiMode();
  const kernelModeEnabled = uiMode === UiMode.Kernel && user?.role?.toLowerCase() === 'developer';

  useSessionExpiryHandler();

  useEffect(() => {
    const handler = () => setTenantAccessDenied(true);
    window.addEventListener(TENANT_ACCESS_DENIED_EVENT, handler);
    return () => window.removeEventListener(TENANT_ACCESS_DENIED_EVENT, handler);
  }, []);

  useEffect(() => {
    const handleKernelMode = (event: Event) => {
      const detail = (event as CustomEvent<{ mode?: UiMode }>).detail;
      const nextMode = detail?.mode;
      if (!nextMode) return;
      if (nextMode === UiMode.Kernel && user?.role?.toLowerCase() !== 'developer') {
        return;
      }
      setUiMode(nextMode);
    };
    window.addEventListener('aos:set-ui-mode', handleKernelMode as EventListener);
    return () => window.removeEventListener('aos:set-ui-mode', handleKernelMode as EventListener);
  }, [setUiMode, user?.role]);

  useEffect(() => {
    if (uiMode === UiMode.Kernel && user?.role?.toLowerCase() !== 'developer') {
      setUiMode(UiMode.User);
    }
  }, [setUiMode, uiMode, user?.role]);

  // Generate navigation groups from centralized route config
  const demoMode = isDemoMvpMode(sessionMode);
  const navigationGroups = useMemo(
    () => generateNavigationGroups(user?.role, user?.permissions, uiMode, { demoMode }),
    [demoMode, user?.permissions, user?.role, uiMode],
  );

  // Convert navigation groups to command items for command palette
  const commandItems: CommandItem[] = useMemo(() => {
    const items: CommandItem[] = [];
    for (const group of navigationGroups) {
      if (!shouldShowNavGroup(group, user?.role)) {
        continue;
      }
      for (const item of group.items) {
        items.push({
          id: `route-${item.to}`,
          type: 'page',
          title: item.label,
          url: item.to,
          icon: item.icon,
          group: group.title,
        });
      }
    }
    items.push(
      {
        id: 'action-open-notifications',
        type: 'action',
        title: 'Open Notification Center',
        description: 'Review unread alerts and system activity',
        actionId: 'open-notifications',
        group: 'Quick Actions',
        shortcut: '⌘⇧N',
      },
      {
        id: 'action-open-help',
        type: 'action',
        title: 'Open Help Center',
        description: 'Browse documentation and shortcut references',
        actionId: 'open-help',
        group: 'Quick Actions',
        shortcut: '?',
      },
      {
        id: 'action-export-adapters',
        type: 'action',
        title: 'Export Adapter Manifests',
        description: 'Open the adapters export dialog',
        actionId: 'open-adapter-export',
        group: 'Quick Actions',
        shortcut: '⌘⇧E',
      }
    );
    if (user?.role?.toLowerCase() === 'developer') {
      const kernelActive = uiMode === UiMode.Kernel;
      items.push({
        id: kernelActive ? 'action-exit-kernel' : 'action-enter-kernel',
        type: 'action',
        title: kernelActive ? 'Exit Kernel Mode' : 'Enter Kernel Mode',
        description: kernelActive
          ? 'Return to Builder view without Kernel overlays'
          : 'Expose Kernel status bar, receipts, Q15 rank, and hot-swap controls',
        actionId: kernelActive ? 'exit-kernel-mode' : 'enter-kernel-mode',
        group: 'Developer',
        shortcut: '⌘⌥K',
      });
    }
    return items;
  }, [navigationGroups, uiMode, user?.role]);

  const requiresTenantSelection = useMemo(() => {
    if (!user) return false;
    const multipleTenants = tenants.length > 1;
    const noTenantAccess = tenants.length === 0;
    const hasSelection = Boolean(selectedTenant && tenants.some((t) => t.id === selectedTenant));
    let forcedSelection = false;
    try {
      forcedSelection = sessionStorage.getItem(TENANT_SELECTION_REQUIRED_KEY) === '1';
    } catch {
      forcedSelection = false;
    }
    return noTenantAccess || (multipleTenants && !hasSelection && (forcedSelection || !selectedTenant));
  }, [selectedTenant, tenants, user]);

  const handleTenantChoice = useCallback(async (tenantId: string) => {
    if (isSwitchingTenant) return;
    setIsSwitchingTenant(true);
    setTenantError(null);
    const ok = await setSelectedTenant(tenantId);
    if (!ok) {
      setTenantError('Unable to switch tenant. You may not have access.');
    } else {
      try {
        sessionStorage.removeItem(TENANT_SELECTION_REQUIRED_KEY);
      } catch {
        // ignore storage errors
      }
      setTenantAccessDenied(false);
    }
    setIsSwitchingTenant(false);
  }, [isSwitchingTenant, setSelectedTenant]);

  // Show loading state with skeleton layout that includes Outlet
  // This prevents blank pages during auth check while preserving route rendering
  if (isLoading) {
    return (
      <CommandPaletteProvider routes={[]}>
        <SidebarProvider>
          <Sidebar collapsible="icon">
            <SidebarContent id="navigation" className="pt-2" role="navigation" aria-label="Main navigation">
              <div className="animate-pulse space-y-3 p-2">
                <div className="h-4 bg-muted rounded w-3/4" />
                <div className="h-4 bg-muted rounded w-1/2" />
                <div className="h-4 bg-muted rounded w-2/3" />
              </div>
            </SidebarContent>
          </Sidebar>
          <SidebarInset>
            <header className="flex items-center gap-2 border-b bg-background/80 backdrop-blur-sm sticky top-0 z-10 px-[var(--space-2)] h-[calc(var(--base-unit)*12)]">
              <Lock className="h-4 w-4 text-primary animate-pulse" />
              <span className="font-medium text-sm text-muted-foreground">Loading...</span>
            </header>
            <main id="main-content" className="flex-1 p-[var(--space-4)] md:p-[var(--space-6)]" role="main" tabIndex={-1}>
              <div className="mx-auto max-w-[var(--layout-content-width-xl)]">
                <Outlet />
              </div>
            </main>
          </SidebarInset>
        </SidebarProvider>
      </CommandPaletteProvider>
    );
  }

const POST_LOGIN_REDIRECT_KEY = 'postLoginRedirect';

// Redirect unauthenticated users to login
if (!user && location.pathname !== '/login') {
  try {
    sessionStorage.setItem(
      POST_LOGIN_REDIRECT_KEY,
      `${location.pathname}${location.search || ''}`,
    );
  } catch {
    // ignore storage errors
  }
  return <Navigate to="/login" replace />;
}

  // Login page without sidebar/navigation - LoginForm handles its own layout
  if (location.pathname === '/login') {
    return (
      <>
        <Outlet />
        <Toaster position="top-right" className="z-[60]" />
      </>
    );
  }

  if (user && !tenantsLoading && requiresTenantSelection) {
    return (
      <>
        <div className="min-h-screen flex items-center justify-center bg-gradient-to-b from-background via-background to-muted/30 px-4 py-10">
          <Card className="w-full max-w-3xl border-border/70 shadow-2xl">
            <CardHeader className="space-y-2">
              <CardTitle className="text-2xl">Select a tenant</CardTitle>
              <CardDescription className="text-base">
                Pick one tenant for this session. You can switch later from the header.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              {tenantError && <div className="text-sm text-destructive">{tenantError}</div>}
              {tenants.length > 0 ? (
                <div className="grid gap-3 sm:grid-cols-2">
                  {tenants.map((tenant) => {
                    const isActive = tenant.id === selectedTenant;
                    return (
                      <Button
                        key={tenant.id}
                        variant={isActive ? 'default' : 'outline'}
                        className={cn(
                          'w-full justify-between items-start text-left h-auto px-4 py-3',
                          isActive && 'shadow-inner'
                        )}
                        disabled={isSwitchingTenant}
                        onClick={() => void handleTenantChoice(tenant.id)}
                      >
                        <div className="flex flex-col gap-1 overflow-hidden">
                          <span className="font-semibold truncate">{tenant.name}</span>
                          <span className="text-xs text-muted-foreground truncate">{tenant.id}</span>
                        </div>
                        <Badge variant={isActive ? 'secondary' : 'outline'} className="flex items-center gap-1">
                          {isActive ? <CheckCircle2 className="h-3 w-3" /> : <ArrowRight className="h-3 w-3" />}
                          {isActive ? 'Active' : 'Select'}
                        </Badge>
                      </Button>
                    );
                  })}
                </div>
              ) : (
                <div className="rounded-md border border-border/80 bg-muted/30 p-3 text-sm text-muted-foreground">
                  You’re signed in but have no tenant access. Ask an admin to grant access or sign out.
                </div>
              )}
            </CardContent>
            <CardFooter className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <div className="text-sm text-muted-foreground">
                Session is paused until you pick a tenant.
              </div>
              <div className="flex w-full gap-2 sm:w-auto">
                <Button
                  variant="outline"
                  onClick={() => void refreshTenants()}
                  disabled={isSwitchingTenant}
                  className="flex-1 sm:flex-none"
                >
                  <RefreshCw className="mr-2 h-4 w-4" />
                  Reload tenants
                </Button>
                <Button
                  variant="ghost"
                  onClick={() => void logout()}
                  disabled={isSwitchingTenant}
                  className="flex-1 sm:flex-none"
                >
                  <LogOut className="mr-2 h-4 w-4" />
                  Sign out
                </Button>
              </div>
          </CardFooter>
        </Card>
      </div>
      <Toaster position="top-right" className="z-[60]" />
    </>
  );
}

  return (
    <LiveDataStatusProvider>
      <KernelTelemetryProvider tenantId={selectedTenant || 'default'}>
        <CopilotProvider>
          <CommandPaletteProvider routes={commandItems}>
            <SidebarProvider>
              <RootLayoutContent
              navigationGroups={navigationGroups}
              tenantAccessDenied={tenantAccessDenied}
              clearTenantAccessDenied={() => setTenantAccessDenied(false)}
              uiMode={uiMode}
              onChangeUiMode={setUiMode}
              isKernelMode={kernelModeEnabled}
            />
          </SidebarProvider>
        </CommandPaletteProvider>
      </CopilotProvider>
      </KernelTelemetryProvider>
    </LiveDataStatusProvider>
  );
}
