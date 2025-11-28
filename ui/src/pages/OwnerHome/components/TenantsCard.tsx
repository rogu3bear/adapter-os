/**
 * Tenants Card Component
 *
 * Displays tenant summary for the Owner Home page:
 * - Total tenant counts by status (active, paused, archived)
 * - Top 3 tenants by usage
 * - Link to detailed tenant management
 */

import React from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { Progress } from '@/components/ui/progress';
import { Building2, Users, ExternalLink } from 'lucide-react';

export interface Tenant {
  id: string;
  name?: string;
  status?: string;
  adapter_count?: number;
  usage_bytes?: number;
}

export interface TenantsCardProps {
  tenants: Tenant[];
  isLoading: boolean;
}

/**
 * Format bytes to human-readable string
 */
function formatBytes(bytes?: number): string {
  if (!bytes || bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

/**
 * Get badge variant based on tenant status
 */
function getStatusVariant(status?: string): 'default' | 'secondary' | 'outline' {
  switch (status) {
    case 'active':
      return 'default';
    case 'paused':
      return 'secondary';
    case 'archived':
      return 'outline';
    default:
      return 'secondary';
  }
}

export default function TenantsCard({ tenants, isLoading }: TenantsCardProps) {
  const navigate = useNavigate();

  // Calculate status counts
  const activeTenants = tenants.filter((t) => t.status === 'active').length;
  const pausedTenants = tenants.filter((t) => t.status === 'paused').length;
  const archivedTenants = tenants.filter((t) => t.status === 'archived').length;
  const totalTenants = tenants.length;

  // Get top 3 tenants by usage
  const topTenants = [...tenants]
    .filter((t) => t.usage_bytes && t.usage_bytes > 0)
    .sort((a, b) => (b.usage_bytes || 0) - (a.usage_bytes || 0))
    .slice(0, 3);

  // Calculate max usage for progress bars
  const maxUsage = topTenants.length > 0 ? topTenants[0].usage_bytes || 0 : 0;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Building2 className="h-5 w-5" />
          Tenants
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-6">
        {isLoading ? (
          <>
            <div className="grid grid-cols-3 gap-4">
              <Skeleton className="h-16 w-full" />
              <Skeleton className="h-16 w-full" />
              <Skeleton className="h-16 w-full" />
            </div>
            <Skeleton className="h-20 w-full" />
            <Skeleton className="h-10 w-full" />
          </>
        ) : (
          <>
        {/* Status Summary */}
        <div className="grid grid-cols-3 gap-4">
          <div className="text-center p-3 bg-slate-50 rounded-lg">
            <div className="text-2xl font-bold text-green-600">
              {activeTenants}
            </div>
            <div className="text-xs text-slate-600 mt-1">Active</div>
          </div>
          <div className="text-center p-3 bg-slate-50 rounded-lg">
            <div className="text-2xl font-bold text-amber-600">
              {pausedTenants}
            </div>
            <div className="text-xs text-slate-600 mt-1">Paused</div>
          </div>
          <div className="text-center p-3 bg-slate-50 rounded-lg">
            <div className="text-2xl font-bold text-slate-400">
              {archivedTenants}
            </div>
            <div className="text-xs text-slate-600 mt-1">Archived</div>
          </div>
        </div>

        {/* Total Count */}
        <div className="flex items-center justify-between pt-2 border-t">
          <div className="flex items-center gap-2 text-sm">
            <Users className="h-4 w-4 text-slate-500" />
            <span className="font-medium">Total Tenants:</span>
          </div>
          <span className="text-lg font-bold">{totalTenants}</span>
        </div>

        {/* Top Tenants by Usage */}
        {topTenants.length > 0 ? (
          <div className="space-y-3">
            <div className="text-sm font-medium text-slate-700">
              Top Tenants by Usage
            </div>
            {topTenants.map((tenant) => {
              const usagePercent = maxUsage > 0
                ? ((tenant.usage_bytes || 0) / maxUsage) * 100
                : 0;

              return (
                <div key={tenant.id} className="space-y-1">
                  <div className="flex items-center justify-between text-sm">
                    <div className="flex items-center gap-2 flex-1 min-w-0">
                      <span className="font-medium truncate">
                        {tenant.name || tenant.id}
                      </span>
                      {tenant.status && (
                        <Badge
                          variant={getStatusVariant(tenant.status)}
                          className="text-xs"
                        >
                          {tenant.status}
                        </Badge>
                      )}
                    </div>
                    <div className="flex items-center gap-3 text-xs text-slate-600">
                      <span>{tenant.adapter_count || 0} adapters</span>
                      <span className="font-medium">
                        {formatBytes(tenant.usage_bytes)}
                      </span>
                    </div>
                  </div>
                  <Progress value={usagePercent} className="h-1.5" />
                </div>
              );
            })}
          </div>
        ) : (
          <div className="text-sm text-slate-500 text-center py-4">
            No usage data available
          </div>
        )}

        {/* View All Button */}
        <Button
          variant="outline"
          className="w-full"
          onClick={() => navigate('/admin/tenants')}
        >
          View All Tenants
          <ExternalLink className="ml-2 h-4 w-4" />
        </Button>
          </>
        )}
      </CardContent>
    </Card>
  );
}
