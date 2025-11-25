import React from 'react';
import { useAuth } from '@/providers/CoreProviders';

interface DashboardLayoutProps {
  children: React.ReactNode;
  title: string;
  quickActions?: React.ReactNode;
}

export default function DashboardLayout({ children, title, quickActions }: DashboardLayoutProps) {
  const { user } = useAuth();

  return (
    <div className="min-h-screen bg-slate-50">
      {/* Skip links for keyboard navigation */}
      <a
        href="#main-content"
        className="sr-only focus:not-sr-only focus:absolute focus:top-4 focus:left-4 focus:z-50 focus:px-4 focus:py-2 focus:bg-primary focus:text-primary-foreground focus:rounded-md"
      >
        Skip to main content
      </a>
      <a
        href="#quick-actions"
        className="sr-only focus:not-sr-only focus:absolute focus:top-4 focus:left-40 focus:z-50 focus:px-4 focus:py-2 focus:bg-primary focus:text-primary-foreground focus:rounded-md"
      >
        Skip to quick actions
      </a>

      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-4 sm:py-6 lg:py-8">
        {/* Header with landmark role */}
        <header role="banner" className="mb-6 sm:mb-8">
          <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <h1 className="text-2xl sm:text-3xl font-bold text-slate-900">{title}</h1>
              <p className="mt-1 text-xs sm:text-sm text-slate-600" aria-live="polite">
                Welcome back, {user?.name || 'User'} ({user?.role || 'viewer'})
              </p>
            </div>
            {quickActions && (
              <nav
                id="quick-actions"
                role="navigation"
                aria-label="Quick actions"
                className="flex flex-col sm:flex-row gap-2 sm:gap-2 md:flex-row"
              >
                {quickActions}
              </nav>
            )}
          </div>
        </header>

        {/* Main content with landmark role */}
        <main
          id="main-content"
          role="main"
          aria-label={`${title} main content`}
          className="space-y-4 sm:space-y-6"
        >
          {children}
        </main>
      </div>
    </div>
  );
}
