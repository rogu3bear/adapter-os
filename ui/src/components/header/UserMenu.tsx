import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Cpu, LogOut } from 'lucide-react';
import { cn } from '@/lib/utils';
import { UiMode } from '@/config/ui-mode';

interface UserMenuProps {
  email: string;
  role: string;
  uiMode?: UiMode;
  onChangeUiMode?: (mode: UiMode) => void;
  onLogout: () => void;
  className?: string;
}

function getInitials(email: string): string {
  const name = email.split('@')[0];
  return name.slice(0, 2).toUpperCase();
}

export function UserMenu({ email, role, uiMode, onChangeUiMode, onLogout, className }: UserMenuProps) {
  const initials = getInitials(email);
  const isDeveloper = role?.toLowerCase() === 'developer';
  const kernelActive = uiMode === UiMode.Kernel;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className={cn('h-10 w-10 p-0', className)}
          data-cy="user-menu-trigger"
          aria-label="User menu"
        >
          <Avatar className="h-8 w-8">
            <AvatarFallback className="text-xs font-medium">{initials}</AvatarFallback>
          </Avatar>
          <span className="sr-only">User menu</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        <DropdownMenuLabel>
          <div className="flex flex-col gap-1">
            <span className="text-sm font-medium">{email}</span>
            <Badge variant="secondary" className="w-fit text-xs">
              {role}
            </Badge>
          </div>
        </DropdownMenuLabel>
        {isDeveloper && onChangeUiMode && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              onClick={() => onChangeUiMode(kernelActive ? UiMode.Builder : UiMode.Kernel)}
              className="flex items-center justify-between text-foreground"
              data-cy="kernel-mode-toggle"
            >
              <div className="flex items-center gap-2">
                <Cpu className="h-4 w-4" />
                <span>{kernelActive ? 'Exit Kernel Mode' : 'Enter Kernel Mode'}</span>
              </div>
              <Badge variant={kernelActive ? 'default' : 'outline'} className="text-[10px] uppercase">
                Kernel
              </Badge>
            </DropdownMenuItem>
          </>
        )}
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={onLogout} className="text-destructive" data-cy="logout-action">
          <LogOut className="mr-2 h-4 w-4" />
          Logout
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
