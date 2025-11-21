import React, { useState, useEffect, useMemo } from 'react';
import { Outlet, useLocation, useNavigate, Navigate } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Toaster } from '@/components/ui/sonner';
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

import { useTheme, useAuth } from '@/providers/CoreProviders';
import { useTenant } from '@/providers/FeatureProviders';
import { CommandPaletteProvider, type CommandItem, useCommandPalette } from '@/contexts/CommandPaletteContext';
import { CommandPalette } from '@/components/CommandPalette';
import { HelpCenter } from '@/components/HelpCenter';
import { NotificationBell } from '@/components/NotificationBell';
import { NotificationCenter } from '@/components/NotificationCenter';
import { useKeyboardShortcuts } from '@/utils/accessibility';
import { generateNavigationGroups, shouldShowNavGroup } from '@/utils/navigation';
import { cn } from '@/components/ui/utils';
import { Lock, ChevronDown, ChevronRight, HelpCircle } from 'lucide-react';

const COLLAPSED_GROUPS_KEY = 'aos_sidebar_collapsed_groups';

interface RootLayoutContentProps {
  navigationGroups: ReturnType<typeof generateNavigationGroups>;
}

function RootLayoutContent({ navigationGroups }: RootLayoutContentProps) {
  const { theme, toggleTheme } = useTheme();
  const { user, logout } = useAuth();
  const { selectedTenant, setSelectedTenant, tenants } = useTenant();
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
    } catch {
      return {};
    }
  });

  const toggleGroup = (groupTitle: string) => {
    setCollapsedGroups(prev => {
      const next = { ...prev, [groupTitle]: !prev[groupTitle] };
      try {
        localStorage.setItem(COLLAPSED_GROUPS_KEY, JSON.stringify(next));
      } catch {
        // Ignore storage errors
      }
      return next;
    });
  };

  return (
    <>
      {/* Sidebar */}
      <Sidebar>
        <SidebarContent className="pt-4">
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
        <header className="border-b bg-card sticky top-0 z-10">
          <div className="flex h-16 items-center justify-between px-4 md:px-6">
            <div className="flex items-center gap-3">
              <SidebarTrigger className="md:hidden" />
              <div className="flex items-center gap-2">
                <Lock className="h-5 w-5 text-primary" />
                <h1 className="font-medium">AdapterOS Control Plane</h1>
              </div>
              <Badge variant="outline" className="text-xs hidden sm:inline-flex">Zero Egress Mode</Badge>
            </div>
            <div className="flex items-center gap-2">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setHelpCenterOpen(true)}
                aria-label="Open help"
                title="Help (?)"
              >
                <HelpCircle className="h-5 w-5" />
              </Button>
              <NotificationBell
                onOpenChange={setNotificationCenterOpen}
                showCountLabel
              />
              {tenants.length > 0 && (
                <Select value={selectedTenant} onValueChange={setSelectedTenant}>
                  <SelectTrigger className="w-[180px] hidden sm:flex">
                    <SelectValue placeholder="Select tenant" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="default">Default</SelectItem>
                    {tenants.filter(t => t.id && t.id !== '').map((t) => (
                      <SelectItem key={t.id} value={t.id}>{t.name}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              )}
              <Badge variant="secondary" className="hidden sm:inline-flex">{user.role}</Badge>
              <span className="text-muted-foreground hidden md:inline">{user.email}</span>
              <Button variant="outline" size="sm" onClick={toggleTheme} aria-label="Toggle theme">
                {theme === 'dark' ? 'Light' : 'Dark'}
              </Button>
              <Button variant="outline" size="sm" onClick={() => void logout()} className="hidden sm:inline-flex">Logout</Button>
            </div>
          </div>
        </header>

        {/* Content */}
        <main className="flex-1 p-4 md:p-6">
          <div className="mx-auto max-w-[1440px]">
            <Outlet />
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
  const navigationGroups = useMemo(() => generateNavigationGroups(user?.role), [user?.role]);

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

  // Show loading state
  if (isLoading) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <div className="text-center">
          <Lock className="h-8 w-8 text-primary mx-auto mb-4 animate-pulse" />
          <p className="text-muted-foreground">Loading...</p>
        </div>
      </div>
    );
  }

  // Redirect unauthenticated users to login
  if (!user && location.pathname !== '/login') {
    return <Navigate to="/login" replace />;
  }

  // Login page without sidebar/navigation
  if (location.pathname === '/login') {
    return (
      <div className="min-h-screen bg-background p-6">
        <Outlet />
        <Toaster position="top-right" className="z-40" />
      </div>
    );
  }

  return (
    <CommandPaletteProvider routes={commandItems}>
      <SidebarProvider>
        <RootLayoutContent navigationGroups={navigationGroups} />
      </SidebarProvider>
    </CommandPaletteProvider>
  );
}
