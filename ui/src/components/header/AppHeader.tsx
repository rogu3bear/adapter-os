import { AlertTriangle, Building2, Check, Loader2 } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { SidebarTrigger } from '@/components/ui/sidebar';
import { Tooltip, TooltipTrigger, TooltipContent } from '@/components/ui/tooltip';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { NotificationBell } from '@/components/NotificationBell';
import { cn } from '@/lib/utils';
import { useTenant } from '@/providers/FeatureProviders';
import { useState } from 'react';
import { UiMode, UI_MODE_OPTIONS } from '@/config/ui-mode';
import type { SessionMode } from '@/api/auth-types';
import { isDemoEnvEnabled } from '@/config/demo';

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
  sessionMode: SessionMode;
  theme: Theme;
  onLogout: () => void;
  onOpenHelp: () => void;
  onOpenNotifications: (open: boolean) => void;
  onOpenPalette: () => void;
  onToggleTheme: () => void;
  className?: string;
  uiMode: UiMode;
  onChangeUiMode: (mode: UiMode) => void;
}

export function AppHeader({
  user,
  sessionMode,
  theme,
  onLogout,
  onOpenHelp,
  onOpenNotifications,
  onOpenPalette,
  onToggleTheme,
  className,
  uiMode,
  onChangeUiMode,
}: AppHeaderProps) {
  const isDemo = sessionMode === 'dev_bypass';
  const demoMode = isDemo || isDemoEnvEnabled();
  const devEnv = Boolean(import.meta.env.DEV);
  const envLabel = isDemo ? 'Demo' : devEnv ? 'Dev' : null;
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
  const { selectedTenant, tenants, setSelectedTenant } = useTenant();
  const [isSwitching, setIsSwitching] = useState(false);
  const activeTenant = tenants.find(t => t.id === selectedTenant);
  const tenantLabel = activeTenant?.name || selectedTenant || 'No tenant';
  const modeLabel: Record<UiMode, string> = {
    [UiMode.User]: 'User',
    [UiMode.Builder]: 'Builder',
    [UiMode.Kernel]: 'Kernel',
    [UiMode.Audit]: 'Audit',
  };
  const isDeveloperProfile = user.role?.toLowerCase() === 'developer';
  const visibleModes = isDeveloperProfile ? UI_MODE_OPTIONS : UI_MODE_OPTIONS.filter(mode => mode !== UiMode.Kernel);

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

          {envLabel && (
            <Tooltip>
              <TooltipTrigger asChild>
                <Badge
                  variant="outline"
                  className="h-5 text-[10px] px-1.5 text-muted-foreground border-muted hidden sm:inline-flex cursor-default"
                  data-testid="env-pill"
                >
                  <AlertTriangle className="h-3 w-3 mr-1" />
                  {envLabel}
                </Badge>
              </TooltipTrigger>
              <TooltipContent className="max-w-xs">
                <div className="space-y-1">
                  <div className="font-medium">{envLabel} environment</div>
                  <div className="text-muted-foreground">
                    API: <code className="font-mono text-xs">{apiBaseUrl}</code>
                  </div>
                  {buildShaShort && (
                    <div className="text-muted-foreground">
                      Commit:{' '}
                      <code className="font-mono text-xs" title={buildSha}>
                        {buildShaShort}
                      </code>
                    </div>
                  )}
                </div>
              </TooltipContent>
            </Tooltip>
          )}

          <span className="text-muted-foreground/30 hidden md:inline">/</span>

          <HeaderBreadcrumbs className="flex-1 min-w-0" />
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                className="inline-flex items-center gap-2 rounded-md border px-2 py-1 text-xs font-medium hover:bg-muted transition-colors"
                data-cy="tenant-switcher"
                data-testid="tenant-switcher"
                aria-label="Tenant switcher"
              >
                <Building2 className="h-4 w-4 text-muted-foreground" />
                <span className="truncate max-w-[140px]">{tenantLabel}</span>
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-64">
              <DropdownMenuLabel className="flex items-center justify-between">
                <span>Tenant</span>
                {isSwitching && <Loader2 className="h-3 w-3 animate-spin text-muted-foreground" />}
              </DropdownMenuLabel>
              <DropdownMenuSeparator />
              {tenants.map(t => (
                <DropdownMenuItem
                  key={t.id}
                  onSelect={async () => {
                    if (t.id === selectedTenant || isSwitching) return;
                    setIsSwitching(true);
                    try {
                      await setSelectedTenant(t.id);
                    } finally {
                      setIsSwitching(false);
                    }
                  }}
                  className="flex items-center justify-between"
                  data-cy="tenant-option"
                  data-tenant-id={t.id}
                  data-testid={`tenant-option-${t.id}`}
                >
                  <span className="truncate">{t.name}</span>
                  {t.id === selectedTenant && <Check className="h-3 w-3 text-primary" />}
                </DropdownMenuItem>
              ))}
              {tenants.length === 0 && (
                <DropdownMenuItem disabled>
                  No tenant access
                </DropdownMenuItem>
              )}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>

        {/* Right: Actions */}
        <div className="flex items-center flex-shrink-0">
          {!demoMode && (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button
                  className="inline-flex items-center gap-2 rounded-md border px-2 py-1 text-xs font-medium hover:bg-muted transition-colors mr-2"
                  data-cy="ui-mode-toggle"
                  aria-label="UI mode toggle"
                >
                  <span className="text-muted-foreground">Mode</span>
                  <Badge variant="secondary" className="text-[11px]">
                    {modeLabel[uiMode]}
                  </Badge>
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-44">
                <DropdownMenuLabel>Interface mode</DropdownMenuLabel>
                <DropdownMenuSeparator />
                {visibleModes.map(mode => (
                  <DropdownMenuItem
                    key={mode}
                    onSelect={() => onChangeUiMode(mode)}
                    className="flex items-center justify-between capitalize"
                    data-cy={`ui-mode-option-${mode}`}
                  >
                    <span>{modeLabel[mode]}</span>
                    {uiMode === mode && <Check className="h-3 w-3 text-primary" />}
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          )}
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
            uiMode={uiMode}
            onChangeUiMode={onChangeUiMode}
            onLogout={onLogout}
          />
        </div>
      </div>
    </header>
  );
}
