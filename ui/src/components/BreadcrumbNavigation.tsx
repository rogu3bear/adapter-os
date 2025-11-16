import React from 'react';
<<<<<<< HEAD
import { Link } from 'react-router-dom';
import { useBreadcrumbs } from '@/hooks/useBreadcrumbs';
=======
import { useBreadcrumb } from '../contexts/BreadcrumbContext';
>>>>>>> integration-branch
import {
  Breadcrumb,
  BreadcrumbList,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from './ui/breadcrumb';
import { ChevronRight, Home } from 'lucide-react';

<<<<<<< HEAD
/**
 * BreadcrumbNavigation - Stateless breadcrumb component
 * Derives breadcrumbs from current URL pathname using route configuration
 */
export function BreadcrumbNavigation() {
  const breadcrumbs = useBreadcrumbs();

  // Filter out Home since we'll add it explicitly, and non-clickable items
  const displayBreadcrumbs = breadcrumbs.filter(b => b.id !== 'home' && b.href !== '#');

  if (displayBreadcrumbs.length === 0) {
=======
export function BreadcrumbNavigation() {
  const { breadcrumbs } = useBreadcrumb();

  if (breadcrumbs.length === 0) {
>>>>>>> integration-branch
    return null;
  }

  return (
    <Breadcrumb className="mb-4">
      <BreadcrumbList>
        <BreadcrumbItem>
          <BreadcrumbLink href="/" className="flex items-center gap-1">
            <Home className="h-4 w-4" />
            <span className="sr-only">Home</span>
          </BreadcrumbLink>
        </BreadcrumbItem>
        
<<<<<<< HEAD
        {displayBreadcrumbs.map((item, index) => {
          const isLast = index === displayBreadcrumbs.length - 1;
=======
        {breadcrumbs.map((item, index) => {
          const isLast = index === breadcrumbs.length - 1;
>>>>>>> integration-branch
          const Icon = item.icon;
          
          return (
            <React.Fragment key={item.id}>
              <BreadcrumbSeparator>
                <ChevronRight className="h-4 w-4" />
              </BreadcrumbSeparator>
              <BreadcrumbItem>
                {isLast ? (
                  <BreadcrumbPage className="flex items-center gap-1">
                    {Icon && <Icon className="h-4 w-4" />}
                    {item.label}
                  </BreadcrumbPage>
                ) : (
<<<<<<< HEAD
                  <BreadcrumbLink asChild>
                    <Link to={item.href} className="flex items-center gap-1">
                      {Icon && <Icon className="h-4 w-4" />}
                      {item.label}
                    </Link>
=======
                  <BreadcrumbLink 
                    href={item.href || '#'} 
                    className="flex items-center gap-1"
                  >
                    {Icon && <Icon className="h-4 w-4" />}
                    {item.label}
>>>>>>> integration-branch
                  </BreadcrumbLink>
                )}
              </BreadcrumbItem>
            </React.Fragment>
          );
        })}
      </BreadcrumbList>
    </Breadcrumb>
  );
}
