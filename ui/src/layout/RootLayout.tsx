import { useState, useEffect, useMemo } from 'react';
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

import { useTheme, useAuth } from '@/providers/CoreProviders';
import { CommandPaletteProvider, type CommandItem, useCommandPalette } from '@/contexts/CommandPaletteContext';
import { CommandPalette } from '@/components/CommandPalette';
import { HelpCenter } from '@/components/HelpCenter';
import { NotificationCenter } from '@/components/NotificationCenter';
import { useKeyboardShortcuts } from '@/utils/accessibility';
import { generateNavigationGroups, shouldShowNavGroup } from '@/utils/navigation';
import { logger } from '@/utils/logger';
import { cn } from '@/components/ui/utils';
import { Lock, ChevronDown, ChevronRight } from 'lucide-react';
import { LiveDataStatusProvider } from '@/hooks/useLiveDataStatus';
import { ConnectionStatusIndicator } from '@/components/header/ConnectionStatusIndicator';

const COLLAPSED_GROUPS_KEY = 'aos_sidebar_collapsed_groups';

interface RootLayoutContentProps {
  navigationGroups: ReturnType<typeof generateNavigationGroups>;
}

function RootLayoutContent({ navigationGroups }: RootLayoutContentProps) {
  const { theme, toggleTheme } = useTheme();
  const { user, logout } = useAuth();
  const location = useLocation();
  const navigate = useNavigate();
  const { openPalette } = useCommandPalette();
  const { isMobile } = useSidebar();

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
          {navigationGroups.filter(group => shouldShowNavGroup(group, user.role)).map((group) => {
            const isCollapsed = collapsedGroups[group.title];
            return (
              <SidebarGroup key={group.title}>
                <SidebarGroupLabel asChild>
                  <button
                    onClick={() => toggleGroup(group.title)}
                    className={cn(
                      'flex items-center justify-between w-full cursor-pointer',
                      isMobile && 'min-h-[44px] px-3 py-3'
                    )}
                    aria-expanded={!isCollapsed}
                    aria-label={`Toggle ${group.title} menu`}
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
                  <SidebarMenu>
                    {group.items.map((item) => {
                      const Icon = item.icon;
                      const isActive = location.pathname === item.to;
                      return (
                        <SidebarMenuItem key={item.to}>
                          <SidebarMenuButton
                            isActive={isActive}
                            onClick={() => navigate(item.to)}
                            tooltip={item.label}
                            size={isMobile ? 'lg' : 'default'}
                            aria-label={`Navigate to ${item.label}`}
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

      {/* Main content area */}
      <SidebarInset>
        {/* Header */}
        <header className="flex items-center border-b bg-background/80 backdrop-blur-sm sticky top-0 z-10">
          <AppHeader
              user={user}
              theme={theme}
              onLogout={() => void logout()}
              onOpenHelp={() => setHelpCenterOpen(true)}
              onOpenNotifications={setNotificationCenterOpen}
              onOpenPalette={openPalette}
              onToggleTheme={toggleTheme}
              className="flex-1"
            />
          {/* Global connection status indicator */}
          <div className="px-3">
            <ConnectionStatusIndicator />
          </div>
        </header>

        {/* Content */}
        <main id="main-content" className="flex-1 p-4 md:p-6" role="main" tabIndex={-1}>
          <div className="mx-auto max-w-[1440px]">
            <SectionErrorBoundary sectionName="Page Content">
              <Outlet />
            </SectionErrorBoundary>
          </div>
        </main>
      </SidebarInset>

      {/* Toaster at z-40 */}
      <Toaster position="top-right" className="z-40" />

      {/* Live region for screen reader announcements */}
      <div id="sr-announcer" aria-live="polite" aria-atomic="true" className="sr-only" />

      {/* Command Palette */}
      <CommandPalette />
      <NotificationCenter open={notificationCenterOpen} onOpenChange={setNotificationCenterOpen} />
      <HelpCenter open={helpCenterOpen} onOpenChange={setHelpCenterOpen} />
    </>
  );
}

export default function RootLayout() {
  const { user, isLoading } = useAuth();
  const location = useLocation();

  // Generate navigation groups from centralized route config
  const navigationGroups = useMemo(() => generateNavigationGroups(user?.role, user?.permissions), [user?.role, user?.permissions]);

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
    return items;
  }, [navigationGroups, user?.role]);

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
            <header className="flex items-center gap-2 border-b bg-background/80 backdrop-blur-sm sticky top-0 z-10 px-2 h-12">
              <Lock className="h-4 w-4 text-primary animate-pulse" />
              <span className="font-medium text-sm text-muted-foreground">Loading...</span>
            </header>
            <main id="main-content" className="flex-1 p-4 md:p-6" role="main" tabIndex={-1}>
              <div className="mx-auto max-w-[1440px]">
                <Outlet />
              </div>
            </main>
          </SidebarInset>
        </SidebarProvider>
      </CommandPaletteProvider>
    );
  }

  // Redirect unauthenticated users to login
  if (!user && location.pathname !== '/login') {
    return <Navigate to="/login" replace />;
  }

  // Login page without sidebar/navigation - LoginForm handles its own layout
  if (location.pathname === '/login') {
    return (
      <>
        <Outlet />
        <Toaster position="top-right" className="z-40" />
      </>
    );
  }

  return (
    <LiveDataStatusProvider>
      <CommandPaletteProvider routes={commandItems}>
        <SidebarProvider>
          <RootLayoutContent navigationGroups={navigationGroups} />
        </SidebarProvider>
      </CommandPaletteProvider>
    </LiveDataStatusProvider>
  );
}
