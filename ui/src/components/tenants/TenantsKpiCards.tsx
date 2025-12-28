import React from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { KpiGrid } from '@/components/ui/grid';
import { Tenant as ApiTenant } from '@/api/types';
import { Users, CheckCircle, Shield, Database } from 'lucide-react';

export interface TenantsKpiCardsProps {
  tenants: ApiTenant[];
}

export function TenantsKpiCards({ tenants }: TenantsKpiCardsProps) {
  const activeCount = tenants.filter((t) => t.status === 'active').length;
  const itarCount = tenants.filter((t) => t.itarCompliant).length;
  const totalAdapters = tenants.reduce((sum, t) => sum + (Number(t.adapters) || 0), 0);

  return (
    <KpiGrid>
      <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
        <CardContent className="pt-6">
          <div className="flex items-center justify-center">
            <Users className="h-4 w-4 text-blue-600" />
            <div>
              <p className="text-2xl font-bold">{tenants.length}</p>
              <p className="text-xs text-muted-foreground">Total Workspaces</p>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
        <CardContent className="pt-6">
          <div className="flex items-center justify-center">
            <CheckCircle className="h-4 w-4 text-green-600" />
            <div>
              <p className="text-2xl font-bold">{activeCount}</p>
              <p className="text-xs text-muted-foreground">Active</p>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
        <CardContent className="pt-6">
          <div className="flex items-center justify-center">
            <Shield className="h-4 w-4 text-orange-600" />
            <div>
              <p className="text-2xl font-bold">{itarCount}</p>
              <p className="text-xs text-muted-foreground">ITAR Compliant</p>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
        <CardContent className="pt-6">
          <div className="flex items-center justify-center">
            <Database className="h-4 w-4 text-purple-600" />
            <div>
              <p className="text-2xl font-bold">{totalAdapters}</p>
              <p className="text-xs text-muted-foreground">Total Adapters</p>
            </div>
          </div>
        </CardContent>
      </Card>
    </KpiGrid>
  );
}
