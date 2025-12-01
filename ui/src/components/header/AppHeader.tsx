import { AlertTriangle } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { SidebarTrigger } from '@/components/ui/sidebar';
import { Tooltip, TooltipTrigger, TooltipContent } from '@/components/ui/tooltip';
import { NotificationBell } from '@/components/NotificationBell';
import { cn } from '@/components/ui/utils';

import { AdapterOSLogo } from './AdapterOSLogo';
import { HeaderBreadcrumbs } from './HeaderBreadcrumbs';
import { SearchTrigger } from './SearchTrigger';
import { UserMenu } from './UserMenu';
import { HeaderActions } from './HeaderActions';

type Theme = 'light' | 'dark' | 'system';

interface AppHeaderProps {
  user: {
    email: string;
    role: string;
    user_id?: string;
  };
  theme: Theme;
  onLogout: () => void;
  onOpenHelp: () => void;
  onOpenNotifications: (open: boolean) => void;
  onOpenPalette: () => void;
  onToggleTheme: () => void;
  className?: string;
}

export function AppHeader({
  user,
  theme,
  onLogout,
  onOpenHelp,
  onOpenNotifications,
  onOpenPalette,
  onToggleTheme,
  className,
}: AppHeaderProps) {
  const isDevBypass = user.user_id === 'dev-admin-user';

  return (
    <header className={cn('border-b border-border/50 bg-background sticky top-0 z-10', className)}>
      <div className="flex h-12 items-center justify-between px-4">
        {/* Left: Branding + Breadcrumbs */}
        <div className="flex items-center gap-3 min-w-0 flex-1">
          <SidebarTrigger className="h-10 w-10" />

          <div className="flex items-center gap-2 flex-shrink-0">
            <AdapterOSLogo size="md" />
            <span className="text-sm font-medium hidden sm:inline">AdapterOS</span>
          </div>

          <Tooltip>
            <TooltipTrigger asChild>
              <div className="h-1.5 w-1.5 rounded-full bg-green-500 flex-shrink-0 hidden sm:block" />
            </TooltipTrigger>
            <TooltipContent className="max-w-xs">Zero Egress</TooltipContent>
          </Tooltip>

          {isDevBypass && (
            <Badge variant="outline" className="h-5 text-[10px] px-1.5 text-muted-foreground border-muted hidden sm:inline-flex">
              <AlertTriangle className="h-3 w-3 mr-1" />
              Dev
            </Badge>
          )}

          <span className="text-muted-foreground/30 hidden md:inline">/</span>

          <HeaderBreadcrumbs className="flex-1 min-w-0" />
        </div>

        {/* Right: Actions */}
        <div className="flex items-center flex-shrink-0">
          <SearchTrigger onClick={onOpenPalette} className="hidden sm:flex" />
          <HeaderActions
            onOpenHelp={onOpenHelp}
            theme={theme}
            onToggleTheme={onToggleTheme}
          />
          <NotificationBell onOpenChange={onOpenNotifications} />
          <UserMenu
            email={user.email}
            role={user.role}
            onLogout={onLogout}
          />
        </div>
      </div>
    </header>
  );
}
