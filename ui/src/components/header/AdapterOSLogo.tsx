import { cn } from '@/components/ui/utils';

interface AdapterOSLogoProps {
  className?: string;
  size?: 'sm' | 'md' | 'lg';
}

export function AdapterOSLogo({ className, size = 'md' }: AdapterOSLogoProps) {
  const sizeClasses = {
    sm: 'h-5 w-5',
    md: 'h-6 w-6',
    lg: 'h-8 w-8',
  };

  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={cn(sizeClasses[size], className)}
    >
      {/* Outer ring - represents the OS/system */}
      <circle
        cx="12"
        cy="12"
        r="10"
        stroke="currentColor"
        strokeWidth="1.5"
        className="text-primary"
      />
      {/* Inner connector nodes - represents adapters/connections */}
      <circle cx="12" cy="6" r="2" fill="currentColor" className="text-primary" />
      <circle cx="6" cy="15" r="2" fill="currentColor" className="text-primary" />
      <circle cx="18" cy="15" r="2" fill="currentColor" className="text-primary" />
      {/* Connecting lines - represents data flow */}
      <path
        d="M12 8v4M10 12l-3 2M14 12l3 2"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        className="text-primary"
      />
      {/* Center hub */}
      <circle cx="12" cy="12" r="2" fill="currentColor" className="text-primary" />
    </svg>
  );
}
