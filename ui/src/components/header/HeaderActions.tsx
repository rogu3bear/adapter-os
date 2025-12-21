import { Button } from '@/components/ui/button';
import { Tooltip, TooltipTrigger, TooltipContent } from '@/components/ui/tooltip';
import { HelpCircle, Sun, Moon, Bug } from 'lucide-react';
import { cn } from '@/lib/utils';
import { useErrorStoreSafe } from '@/stores/errorStore';
import { useNavigate } from 'react-router-dom';

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
  const errorStore = useErrorStoreSafe();
  const errorCount = errorStore?.getActiveCount() ?? 0;
  const navigate = useNavigate();

  return (
    <div className={cn('flex items-center', className)}>
      {import.meta.env.DEV && (
        <Button
          variant="ghost"
          size="icon"
          onClick={() => navigate('/dev/api-errors')}
          className="relative h-10 w-10"
          title="Dev Error Inspector"
        >
          <Bug className="h-4 w-4" />
          {errorCount > 0 && (
            <span className="absolute -top-1 -right-1 flex h-4 w-4 items-center justify-center rounded-full bg-red-500 text-[10px] text-white">
              {errorCount > 9 ? '9+' : errorCount}
            </span>
          )}
        </Button>
      )}

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
        <TooltipContent className="max-w-xs">Help (?)</TooltipContent>
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
        <TooltipContent className="max-w-xs">
          {theme === 'dark' ? 'Light mode' : 'Dark mode'}
        </TooltipContent>
      </Tooltip>
    </div>
  );
}
