// 【ui/src/components/ui/use-mobile.ts】 - Mobile detection hook
// 【ui/src/components/MobileNavigation.tsx】 - Mobile navigation component
import React, { useState, useEffect, useMemo } from 'react';
import { Outlet, Link, useLocation, useNavigate } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Toaster } from '@/components/ui/sonner';
import { useAuth } from '@/providers/CoreProviders';
import { useTenant } from '@/providers/FeatureProviders';
import { Navigate } from 'react-router-dom';
import { useIsMobile } from '@/components/ui/use-mobile';
import { MobileNavigation } from '@/components/MobileNavigation';
import { CommandPaletteProvider, type CommandItem, useCommandPalette } from '@/contexts/CommandPaletteContext';
import { CommandPalette } from '@/components/CommandPalette';
import { HelpCenter } from '@/components/HelpCenter';
import { NotificationBell } from '@/components/NotificationBell';
import { NotificationCenter } from '@/components/NotificationCenter';
import { useKeyboardShortcuts } from '@/utils/accessibility';
import { generateNavigationGroups, shouldShowNavGroup, type NavGroup } from '@/utils/navigation';
import {
  Lock,
  Menu,
  X,
  Compass,
  LayoutDashboard,
  Zap,
  FlaskConical,
  GitCompare,
  TrendingUp,
  Box,
  Route,
  Play,
  Activity,
  Shield,
  Eye,
  RotateCcw,
  FileText,
  Building,
  ChevronDown,
  ChevronRight,
  Settings,
  BarChart3,
  Upload,
  HelpCircle
} from 'lucide-react';
import type { UserRole } from '@/api/types';
import { cn, FROST_OVERLAY } from '@/components/ui/utils';

function RootLayoutContent() {
  // All hooks must be called before any conditional returns
  const { user, isLoading, logout } = useAuth();
  const { selectedTenant, setSelectedTenant, tenants } = useTenant();
  const [mobileMenuOpen, setMobileMenuOpen] = React.useState(false);
  const location = useLocation();
  const navigate = useNavigate();
  const [isSidebarOpen, setIsSidebarOpen] = useState(false);
  const [helpCenterOpen, setHelpCenterOpen] = useState(false);
  const [notificationCenterOpen, setNotificationCenterOpen] = useState(false);
  const isMobile = useIsMobile();
  const { openPalette } = useCommandPalette();

  // Wire up keyboard shortcuts
  useKeyboardShortcuts({
    onSearch: openPalette,
    onHelp: () => setHelpCenterOpen(true),
    onNotifications: () => setNotificationCenterOpen(true),
  });

  useEffect(() => {
    if (isSidebarOpen) {
      document.body.classList.add('overflow-hidden');
    } else {
      document.body.classList.remove('overflow-hidden');
    }
    return () => document.body.classList.remove('overflow-hidden');
  }, [isSidebarOpen]);

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

  const toggleSidebar = () => setIsSidebarOpen(!isSidebarOpen);

  React.useEffect(() => { setMobileMenuOpen(false); }, [location.pathname]);

  // Generate navigation groups from centralized route config
  const navigationGroups = useMemo(() => generateNavigationGroups(user?.role), [user?.role]);

  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>({});

  const toggleGroup = (groupTitle: string) => {
    setCollapsedGroups(prev => ({
      ...prev,
      [groupTitle]: !prev[groupTitle]
    }));
  };

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

  // Handle auth at layout level - redirect unauthenticated users to login
  // Exception: /login route is handled by LoginRoute component
  if (!user && location.pathname !== '/login') {
    return <Navigate to="/login" replace />;
  }

  // Allow login page to render without sidebar/navigation
  if (location.pathname === '/login') {
    return (
      <div className="min-h-screen bg-background p-6">
        <Outlet />
        <Toaster position="top-right" className="z-40" />
      </div>
    );
  }


  return (
    <div className="min-h-screen bg-background">
      {/* Header */}
      <header className="border-b bg-card">
        <div className="flex h-16 items-center justify-between px-4 md:px-6">
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-2">
              <Lock className="h-5 w-5 text-primary" />
              <h1 className="font-medium">AdapterOS Control Plane</h1>
            </div>
            <div className="text-xs rounded px-2 py-0.5 border border-gray-300 text-gray-700 bg-gray-50">Zero Egress Mode</div>
          </div>
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" className="md:hidden" onClick={toggleSidebar} aria-label="Open menu">
              <Menu className="h-5 w-5" />
            </Button>
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
            <Button variant="outline" size="sm" onClick={() => void logout()} className="hidden sm:inline-flex">Logout</Button>
          </div>
        </div>
      </header>

      {/* Overlay for mobile */}
      {isSidebarOpen && <div className={cn("fixed inset-0 z-40", FROST_OVERLAY, "md:hidden")} onClick={() => setIsSidebarOpen(false)} aria-hidden="true" />}

      {/* Body with sidebar and content */}
      <div className="flex min-h-[calc(100vh-4rem)]">
        {/* Sidebar - fixed overlay on mobile, static flex item on desktop */}
        <aside className={`fixed top-0 bottom-0 left-0 z-50 w-64 transform ${isSidebarOpen ? 'translate-x-0' : '-translate-x-full'} transition-transform md:relative md:translate-x-0 md:z-auto md:w-64 md:flex-shrink-0 overflow-y-auto bg-background border-r`}>
          <div className="p-4 space-y-1">
            <Button className="md:hidden mb-4 w-full justify-start" variant="ghost" onClick={() => setIsSidebarOpen(false)} aria-label="Close menu">
              <X className="h-5 w-5 mr-2" />
              Close Menu
            </Button>
            
            {isMobile ? (
              <MobileNavigation 
                groups={navigationGroups.filter(group => shouldShowNavGroup(group, user?.role))}
                onNavigate={(path) => {
                  navigate(path);
                  setIsSidebarOpen(false);
                }}
                userRole={user?.role}
              />
            ) : (
              navigationGroups.filter(group => shouldShowNavGroup(group, user?.role)).map((group) => {
                const isCollapsed = collapsedGroups[group.title];
                return (
                  <div key={group.title} className="mb-4">
                    <button
                      onClick={() => toggleGroup(group.title)}
                      className="flex items-center justify-between w-full px-2 py-1.5 text-xs font-semibold text-muted-foreground uppercase tracking-wider hover:text-foreground transition-colors"
                    >
                      <span>{group.title}</span>
                      {isCollapsed ? (
                        <ChevronRight className="h-3 w-3" />
                      ) : (
                        <ChevronDown className="h-3 w-3" />
                      )}
                    </button>
                    
                    {!isCollapsed && (
                      <div className="mt-1 space-y-1">
                        {group.items.map((item) => {
                          const Icon = item.icon;
                          const isActive = location.pathname === item.to;
                          return (
                            <Button
                              key={item.to}
                              variant={isActive ? 'default' : 'ghost'}
                              className="w-full justify-start"
                              onClick={() => {
                                navigate(item.to);
                                setIsSidebarOpen(false);
                              }}
                              aria-current={isActive ? 'page' : undefined}
                            >
                              <Icon className="h-4 w-4 mr-2" />
                              {item.label}
                            </Button>
                          );
                        })}
                      </div>
                    )}
                  </div>
                );
              })
            )}
          </div>
        </aside>

        {/* Content */}
        <main className="flex-1 p-4 md:p-6">
          <div className="mx-auto max-w-[1440px]">
            <Outlet />
          </div>
        </main>
      </div>

      {/* Toaster at z-40 */}
      <Toaster position="top-right" className="z-40" />
      {/* Live region for screen reader announcements */}
      <div id="sr-announcer" aria-live="polite" aria-atomic="true" className="sr-only" />
      
      {/* Command Palette */}
      <CommandPalette />
      <NotificationCenter open={notificationCenterOpen} onOpenChange={setNotificationCenterOpen} />
      <HelpCenter open={helpCenterOpen} onOpenChange={setHelpCenterOpen} />
    </div>
  );
}

export default function RootLayout() {
  const { user } = useAuth();
  const location = useLocation();

  // Generate navigation groups from centralized route config
  const navigationGroups = useMemo(() => generateNavigationGroups(user?.role), [user?.role]);

  // Convert navigation groups to command items for command palette
  const routes: CommandItem[] = useMemo(() => {
    const items: CommandItem[] = [];
    for (const group of navigationGroups) {
      // Filter by role if needed
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

  // Handle auth at layout level - redirect unauthenticated users to login
  if (!user && location.pathname !== '/login') {
    return <Navigate to="/login" replace />;
  }

  // Allow login page to render without sidebar/navigation
  if (location.pathname === '/login') {
    return (
      <div className="min-h-screen bg-background p-6">
        <Outlet />
        <Toaster position="top-right" className="z-40" />
      </div>
    );
  }

  return (
    <CommandPaletteProvider routes={routes}>
      <RootLayoutContent />
    </CommandPaletteProvider>
  );
}
