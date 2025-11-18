import React from 'react';
import { Outlet, Link, useLocation, useNavigate } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Toaster } from '@/components/ui/sonner';
import { useTheme, useAuth, useTenant } from './LayoutProvider';
import { Lock, Menu, X } from 'lucide-react';

export default function RootLayout() {
  const { theme, toggleTheme } = useTheme();
  const { user, isLoading, logout } = useAuth();
  const { selectedTenant, setSelectedTenant, tenants } = useTenant();
  const [mobileMenuOpen, setMobileMenuOpen] = React.useState(false);
  const location = useLocation();
  const navigate = useNavigate();

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

  const items = [
    { to: '/dashboard', label: 'Dashboard' },
    { to: '/telemetry', label: 'Telemetry' },
    { to: '/alerts', label: 'Alerts' },
    { to: '/replay', label: 'Replay' },
    { to: '/policies', label: 'Policies' },
  ];

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
            <Button variant="ghost" size="sm" className="md:hidden" onClick={() => setMobileMenuOpen(true)} aria-label="Open menu">
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

      {/* Body */}
      <div className="flex">
        {/* Sidebar */}
        <nav className={`w-64 border-r bg-card p-4 md:block ${mobileMenuOpen ? 'fixed inset-y-0 left-0 z-10 block' : 'hidden'}`} aria-label="Main navigation">
          <div className="space-y-2">
            <Button className="md:hidden mb-4 w-full justify-start" variant="ghost" onClick={() => setMobileMenuOpen(false)} aria-label="Close menu">
              <X className="h-5 w-5 mr-2" />
              Close Menu
            </Button>
            {items.map((item) => (
              <Button
                key={item.to}
                variant={location.pathname === item.to ? 'default' : 'ghost'}
                className="w-full justify-start"
                onClick={() => navigate(item.to)}
                aria-current={location.pathname === item.to ? 'page' : undefined}
              >
                {item.label}
              </Button>
            ))}
          </div>
        </nav>

        {/* Mobile overlay */}
        {mobileMenuOpen && (
          <div className="fixed inset-0 bg-black/50 z-10 md:hidden" onClick={() => setMobileMenuOpen(false)} aria-hidden="true" />
        )}

        {/* Content */}
        <main className="flex-1 p-4 md:p-6">
          <div className="mx-auto max-w-[1440px]">
            <Outlet />
          </div>
        </main>
      </div>

      {/* Toaster at z-40 */}
      <Toaster position="top-right" className="z-40" />
    </div>
  );
}


