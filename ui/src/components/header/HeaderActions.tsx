import { Button } from '@/components/ui/button';
import { Tooltip, TooltipTrigger, TooltipContent } from '@/components/ui/tooltip';
import { HelpCircle, Sun, Moon } from 'lucide-react';
import { cn } from '@/components/ui/utils';

type Theme = 'light' | 'dark' | 'system';

interface HeaderActionsProps {
  onOpenHelp: () => void;
  theme: Theme;
  onToggleTheme: () => void;
  className?: string;
}

export function HeaderActions({
  onOpenHelp,
  theme,
  onToggleTheme,
  className,
}: HeaderActionsProps) {
  return (
    <div className={cn('flex items-center', className)}>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            onClick={onOpenHelp}
            className="h-10 w-10"
          >
            <HelpCircle className="h-5 w-5" />
            <span className="sr-only">Help</span>
          </Button>
        </TooltipTrigger>
        <TooltipContent>Help (?)</TooltipContent>
      </Tooltip>

      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            onClick={onToggleTheme}
            className="h-10 w-10"
          >
            {theme === 'dark' ? (
              <Sun className="h-5 w-5" />
            ) : (
              <Moon className="h-5 w-5" />
            )}
            <span className="sr-only">Toggle theme</span>
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          {theme === 'dark' ? 'Light mode' : 'Dark mode'}
        </TooltipContent>
      </Tooltip>
    </div>
  );
}
