// 【ui/src/components/ui/use-mobile.ts】 - Mobile detection hook
// 【ui/src/components/MobileNavigation.tsx】 - Mobile navigation component
import React, { useState, useEffect } from 'react';
import { Outlet, Link, useLocation, useNavigate } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Toaster } from '@/components/ui/sonner';
import { useTheme, useAuth, useTenant } from './LayoutProvider';
import { useIsMobile } from '@/components/ui/use-mobile';
import { MobileNavigation } from '@/components/MobileNavigation';
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
  Upload
} from 'lucide-react';
import type { UserRole } from '@/api/types';

export default function RootLayout() {
  const { theme, toggleTheme } = useTheme();
  const { user, isLoading, logout } = useAuth();
  const { selectedTenant, setSelectedTenant, tenants } = useTenant();
  const [mobileMenuOpen, setMobileMenuOpen] = React.useState(false);
  const location = useLocation();
  const navigate = useNavigate();
  const [isSidebarOpen, setIsSidebarOpen] = useState(false);
  const isMobile = useIsMobile();

  useEffect(() => {
    if (isSidebarOpen) {
      document.body.classList.add('overflow-hidden');
    } else {
      document.body.classList.remove('overflow-hidden');
    }
    return () => document.body.classList.remove('overflow-hidden');
  }, [isSidebarOpen]);

  const toggleSidebar = () => setIsSidebarOpen(!isSidebarOpen);

  React.useEffect(() => { setMobileMenuOpen(false); }, [location.pathname]);

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

  if (!user) {
    // Simple unauthenticated shell (feature pages can render their own forms)
    return (
      <div className="min-h-screen bg-background p-6">{/* Placeholder outlet for login routes if added later */}
        <Outlet />
        <Toaster position="top-right" className="z-40" />
      </div>
    );
  }

  interface NavItem {
    to: string;
    label: string;
    icon: any;
  }

  interface NavGroup {
    title: string;
    items: NavItem[];
    roles?: UserRole[];
  }

  // 【ui/src/layout/RootLayout.tsx§90-141】 - Navigation refactor: User-centric grouping
  const navigationGroups: NavGroup[] = [
    {
      title: 'Home',
      items: [
        { to: '/dashboard', label: 'Dashboard', icon: LayoutDashboard },
        { to: '/workflow', label: 'Getting Started', icon: Compass }
      ]
    },
    {
      title: 'ML Pipeline',
      items: [
        { to: '/trainer', label: 'Single-File Trainer', icon: Upload },
        { to: '/training', label: 'Training Jobs', icon: Zap },
        { to: '/testing', label: 'Testing', icon: FlaskConical },
        { to: '/golden', label: 'Golden Runs', icon: GitCompare },
        { to: '/promotion', label: 'Promotion', icon: TrendingUp },
        { to: '/adapters', label: 'Adapters', icon: Box }
      ]
    },
    {
      title: 'Monitoring',
      items: [
        { to: '/metrics', label: 'Metrics', icon: Activity },
        { to: '/monitoring', label: 'System Health', icon: Activity },
        { to: '/routing', label: 'Routing Inspector', icon: Route }
      ]
    },
    {
      title: 'Operations',
      items: [
        { to: '/inference', label: 'Inference', icon: Play },
        { to: '/telemetry', label: 'Telemetry', icon: Eye },
        { to: '/replay', label: 'Replay', icon: RotateCcw }
      ]
    },
    {
      title: 'Compliance',
      items: [
        { to: '/policies', label: 'Policies', icon: Shield },
        { to: '/audit', label: 'Audit', icon: FileText }
      ]
    },
    {
      title: 'Administration',
      items: [
        { to: '/admin', label: 'IT Admin', icon: Settings },
        { to: '/reports', label: 'Reports', icon: BarChart3 },
        { to: '/tenants', label: 'Tenants', icon: Building }
      ],
      roles: ['Admin']
    }
  ];

  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>({});

  const toggleGroup = (groupTitle: string) => {
    setCollapsedGroups(prev => ({
      ...prev,
      [groupTitle]: !prev[groupTitle]
    }));
  };

  const shouldShowGroup = (group: NavGroup): boolean => {
    if (!group.roles || group.roles.length === 0) return true;
    return user ? group.roles.includes(user.role) : false;
  };

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
            <div className="text-xs rounded px-2 py-0.5 border text-green-700 bg-green-50">Zero Egress Mode</div>
          </div>
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" className="md:hidden" onClick={toggleSidebar} aria-label="Open menu">
              <Menu className="h-5 w-5" />
            </Button>
            {tenants.length > 0 && (
              <Select value={selectedTenant} onValueChange={setSelectedTenant}>
                <SelectTrigger className="w-[180px] hidden sm:flex">
                  <SelectValue placeholder="Select tenant" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="default">Default</SelectItem>
                  {tenants.map((t) => (
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

      {/* Sidebar */}
      <div className={`fixed inset-y-0 left-0 z-50 w-64 transform ${isSidebarOpen ? 'translate-x-0' : '-translate-x-full'} transition-transform md:translate-x-0 md:static md:inset-auto md:w-64 md:shadow-none overflow-y-auto bg-background border-r`}>
        <div className="p-4 space-y-1">
          <Button className="md:hidden mb-4 w-full justify-start" variant="ghost" onClick={() => setIsSidebarOpen(false)} aria-label="Close menu">
            <X className="h-5 w-5 mr-2" />
            Close Menu
          </Button>
          
          {isMobile ? (
            <MobileNavigation 
              groups={navigationGroups.filter(shouldShowGroup)}
              onNavigate={(path) => {
                navigate(path);
                setIsSidebarOpen(false);
              }}
              userRole={user?.role}
            />
          ) : (
            navigationGroups.filter(shouldShowGroup).map((group) => {
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
      </div>

      {/* Overlay for mobile */}
      {isSidebarOpen && <div className="fixed inset-0 z-40 bg-black/50 md:hidden" onClick={() => setIsSidebarOpen(false)} aria-hidden="true" />}

      {/* Body */}
      <div className="flex">
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
    </div>
  );
}
