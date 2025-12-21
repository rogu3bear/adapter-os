import React from 'react';

export type IconName = 
  | 'checkmark'
  | 'cross'
  | 'warning'
  | 'rocket'
  | 'chart'
  | 'search'
  | 'lightning'
  | 'book'
  | 'shield'
  | 'trending-up'
  | 'lock'
  | 'play'
  | 'pause'
  | 'sun'
  | 'moon'
  | 'target'
  | 'fire'
  | 'clipboard'
  | 'file'
  | 'alert-triangle';

interface IconProps {
  name: IconName;
  className?: string;
  size?: number;
}

export const Icon: React.FC<IconProps> = ({ name, className = '', size = 16 }) => {
  const iconComponents: Record<IconName, React.ComponentType<{ className?: string; size?: number }>> = {
    'checkmark': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <path d="M13.5 4.5L6 12L2.5 8.5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
      </svg>
    ),
    'cross': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <path d="M12 4L4 12M4 4L12 12" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
      </svg>
    ),
    'warning': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <path d="M8 1L15 13H1L8 1Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
        <path d="M8 5V9" stroke="currentColor" strokeWidth="2" strokeLinecap="round"/>
        <circle cx="8" cy="11" r="1" fill="currentColor"/>
      </svg>
    ),
    'rocket': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <path d="M4.5 16C4.5 16 5.5 14 8 14C10.5 14 11.5 16 11.5 16" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
        <path d="M8 2L12 6L8 10L4 6L8 2Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
        <path d="M8 6V10" stroke="currentColor" strokeWidth="2" strokeLinecap="round"/>
      </svg>
    ),
    'chart': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <rect x="2" y="8" width="2" height="6" stroke="currentColor" strokeWidth="2" fill="none"/>
        <rect x="6" y="4" width="2" height="10" stroke="currentColor" strokeWidth="2" fill="none"/>
        <rect x="10" y="6" width="2" height="8" stroke="currentColor" strokeWidth="2" fill="none"/>
      </svg>
    ),
    'search': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <circle cx="7" cy="7" r="4" stroke="currentColor" strokeWidth="2"/>
        <path d="M13 13L10.5 10.5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
      </svg>
    ),
    'lightning': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <path d="M8 1L3 9H8L7 15L13 7H8L8 1Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
      </svg>
    ),
    'book': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <path d="M4 2H12C12.5523 2 13 2.44772 13 3V13C13 13.5523 12.5523 14 12 14H4C3.44772 14 3 13.5523 3 13V3C3 2.44772 3.44772 2 4 2Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
        <path d="M7 2V14" stroke="currentColor" strokeWidth="2" strokeLinecap="round"/>
      </svg>
    ),
    'shield': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <path d="M8 1L13 3V8C13 11.5 8 15 8 15C8 15 3 11.5 3 8V3L8 1Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
      </svg>
    ),
    'trending-up': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <path d="M3 10L6 7L9 10L13 6" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
        <path d="M13 6H10V9" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
      </svg>
    ),
    'lock': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <rect x="4" y="7" width="8" height="7" rx="1" stroke="currentColor" strokeWidth="2" fill="none"/>
        <path d="M6 7V4C6 2.89543 6.89543 2 8 2C9.10457 2 10 2.89543 10 4V7" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
      </svg>
    ),
    'play': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <path d="M4 2L12 8L4 14V2Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
      </svg>
    ),
    'pause': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <rect x="4" y="2" width="3" height="12" stroke="currentColor" strokeWidth="2" fill="none"/>
        <rect x="9" y="2" width="3" height="12" stroke="currentColor" strokeWidth="2" fill="none"/>
      </svg>
    ),
    'sun': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <circle cx="8" cy="8" r="3" stroke="currentColor" strokeWidth="2"/>
        <path d="M8 1V3M8 13V15M15 8H13M3 8H1M12.5 3.5L11 5M5 11L3.5 12.5M12.5 12.5L11 11M5 5L3.5 3.5" stroke="currentColor" strokeWidth="2" strokeLinecap="round"/>
      </svg>
    ),
    'moon': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <path d="M14 9C14 12.866 10.866 16 7 16C3.134 16 0 12.866 0 9C0 5.134 3.134 2 7 2C10.866 2 14 5.134 14 9Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
      </svg>
    ),
    'target': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <circle cx="8" cy="8" r="6" stroke="currentColor" strokeWidth="2"/>
        <circle cx="8" cy="8" r="3" stroke="currentColor" strokeWidth="2"/>
        <circle cx="8" cy="8" r="1" fill="currentColor"/>
      </svg>
    ),
    'fire': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <path d="M8 16C8 16 12 12 12 8C12 6 11 4 9 3C9 5 8 6 6 6C4 6 2 4 2 2C0 4 0 8 4 12C4 14 6 16 8 16Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
      </svg>
    ),
    'clipboard': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <rect x="4" y="2" width="8" height="12" rx="1" stroke="currentColor" strokeWidth="2" fill="none"/>
        <path d="M6 1H10C10.5523 1 11 1.44772 11 2V3H5V2C5 1.44772 5.44772 1 6 1Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
      </svg>
    ),
    'file': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <path d="M14 2H6L4 4V14C4 14.5523 4.44772 15 5 15H13C13.5523 15 14 14.5523 14 14V2Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
        <path d="M4 4H6V2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
      </svg>
    ),
    'alert-triangle': ({ className, size }) => (
      <svg width={size} height={size} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
        <path d="M8 1L15 13H1L8 1Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
        <path d="M8 5V9" stroke="currentColor" strokeWidth="2" strokeLinecap="round"/>
        <circle cx="8" cy="11" r="1" fill="currentColor"/>
      </svg>
    ),
  };

  const IconComponent = iconComponents[name];
  return <IconComponent className={className} size={size} />;
};
