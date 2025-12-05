import React, { useMemo, useState, useEffect } from 'react';
import { useLocation, useNavigate, useParams } from 'react-router-dom';
import { useMutation, useQuery } from '@tanstack/react-query';
import FeatureLayout from '@/layout/FeatureLayout';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { AdaptersPage as AdaptersList } from '@/components/AdaptersPage';
import AdapterDetailPage from '@/pages/Adapters/AdapterDetailPage';
import AdapterActivationsPage from '@/pages/Adapters/AdapterActivations';
import AdapterLineagePage from '@/pages/Adapters/AdapterLineage';
import AdapterManifestPage from '@/pages/Adapters/AdapterManifest';
import AdapterRegisterPage from '@/pages/Adapters/AdapterRegisterPage';
import AdapterUsage from '@/pages/Adapters/AdapterUsage';
import { useAdapterDetail } from '@/hooks/useAdapterDetail';
import { adaptersTabOrder, adapterTabToPath, AdaptersTab, resolveAdaptersTab } from '@/pages/Adapters/tabs';
import type { AdapterCategory, AdapterDetailResponse } from '@/api/adapter-types';
import { CANONICAL_POLICIES } from '@/api/policyTypes';
import apiClient from '@/api/client';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Checkbox } from '@/components/ui/checkbox';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { useToast } from '@/hooks/use-toast';
import { Separator } from '@/components/ui/separator';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';

export default function AdaptersShell() {
  const { adapterId } = useParams<{ adapterId: string }>();
  const location = useLocation();
  const navigate = useNavigate();
  const [postRegisterBanner, setPostRegisterBanner] = useState<{ adapterName?: string } | null>(null);

  const basePath = useMemo(() => (adapterId ? `/adapters/${adapterId}` : '/adapters'), [adapterId]);

  const {
    adapter: adapterDetail,
    lineage,
    activations,
    manifest,
    isLoadingLineage,
    isLoadingActivations,
    isLoadingManifest,
    refetchActivations,
    refetchLineage,
    refetchManifest,
  } = useAdapterDetail(adapterId || '', { enabled: !!adapterId });

  const activeTab: AdaptersTab = useMemo(
    () => resolveAdaptersTab(location.pathname, location.hash, adapterId || undefined),
    [adapterId, location.hash, location.pathname],
  );

  const tabPath = (tab: AdaptersTab) => adapterTabToPath(tab, adapterId || undefined);

  useEffect(() => {
    const state = (location.state || {}) as { fromRegister?: boolean; adapterName?: string };
    if (state.fromRegister) {
      setPostRegisterBanner({ adapterName: state.adapterName });
      navigate(`${location.pathname}${location.hash}`, { replace: true, state: {} });
    }
  }, [location.hash, location.pathname, location.state, navigate]);

  return (
    <FeatureLayout
      title="Adapters"
      description="Adapter details and controls"
      customHeader={null}
      maxWidth="xl"
    >
      {postRegisterBanner && (
        <Alert className="mb-4">
          <AlertTitle>Adapter registered</AlertTitle>
          <AlertDescription className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
            <span>
              Next: train this adapter or configure routing.
            </span>
            <div className="flex flex-wrap gap-2">
              <Button size="sm" onClick={() => navigate(`/training/jobs?adapterId=${adapterId}`)}>
                Train adapter
              </Button>
              <Button variant="outline" size="sm" onClick={() => navigate(`/router-config?adapterId=${adapterId}`)}>
                Configure routing
              </Button>
            </div>
          </AlertDescription>
        </Alert>
      )}
      <Tabs
        value={activeTab}
        onValueChange={(value: string) => {
          const tab = value as AdaptersTab;
          const nextPath = tabPath(tab);
          const nextLocation = nextPath.split('#')[0];
          if (nextLocation !== location.pathname || location.hash !== '') {
            navigate(nextPath);
          }
        }}
      >
        <TabsList className="w-full grid grid-cols-3 md:grid-cols-4 lg:grid-cols-7">
          {adaptersTabOrder.map(tab => (
            <TabsTrigger key={tab} value={tab}>
              {tab === 'overview' && 'Overview'}
              {tab === 'activations' && 'Activations'}
              {tab === 'usage' && 'Usage'}
              {tab === 'lineage' && 'Lineage'}
              {tab === 'manifest' && 'Manifest'}
              {tab === 'register' && 'Register'}
              {tab === 'policies' && 'Policies'}
            </TabsTrigger>
          ))}
        </TabsList>

        <TabsContent value="overview" className="mt-6">
          {adapterId ? <AdapterDetailPage /> : <AdaptersList />}
        </TabsContent>
        <TabsContent value="activations" className="mt-6">
          {adapterId ? (
            <AdapterActivationsPage
              adapterId={adapterId}
              activations={activations}
              isLoading={isLoadingActivations}
              onRefresh={refetchActivations}
            />
          ) : (
            <div className="text-sm text-muted-foreground">Select an adapter to view activations.</div>
          )}
        </TabsContent>
        <TabsContent value="usage" className="mt-6">
          <AdapterUsage />
        </TabsContent>
        <TabsContent value="lineage" className="mt-6">
          {adapterId ? (
            <AdapterLineagePage
              adapterId={adapterId}
              lineage={lineage}
              isLoading={isLoadingLineage}
            />
          ) : (
            <div className="text-sm text-muted-foreground">Select an adapter to view lineage.</div>
          )}
        </TabsContent>
        <TabsContent value="manifest" className="mt-6">
          {adapterId ? (
            <AdapterManifestPage
              adapterId={adapterId}
              manifest={manifest}
              isLoading={isLoadingManifest}
            />
          ) : (
            <div className="text-sm text-muted-foreground">Select an adapter to view manifest.</div>
          )}
        </TabsContent>
        <TabsContent value="register" className="mt-6">
          <AdapterRegisterPage />
        </TabsContent>
        <TabsContent value="policies" className="mt-6">
          <AdapterPoliciesTab adapterId={adapterId} adapterDetail={adapterDetail} />
        </TabsContent>
      </Tabs>
    </FeatureLayout>
  );
}

function AdapterPoliciesTab({
  adapterId,
  adapterDetail,
}: {
  adapterId?: string;
  adapterDetail: AdapterDetailResponse | null;
}) {
  const { toast } = useToast();
  const [selectedPolicyIds, setSelectedPolicyIds] = useState<string[]>([]);

  const category = adapterDetail?.adapter?.category as AdapterCategory | undefined;
  const appliedPolicyIds =
    ((adapterDetail?.adapter as unknown as { policy_ids?: string[] })?.policy_ids ?? []) as string[];

  useEffect(() => {
    setSelectedPolicyIds(appliedPolicyIds);
  }, [appliedPolicyIds.join(',')]);

  const { data: categoryPolicy } = useQuery({
    queryKey: ['adapter-category-policy', category],
    queryFn: () => apiClient.getCategoryPolicy(category as AdapterCategory),
    enabled: Boolean(category),
  });

  const updatePolicyMutation = useMutation({
    mutationFn: async (policyIds: string[]) => {
      if (!adapterId) return;
      return apiClient.updateAdapterPolicy(adapterId, { policy_ids: policyIds, category });
    },
    onSuccess: () => {
      toast({
        title: 'Adapter policies updated',
        description: 'Effective policies refreshed for this adapter',
      });
    },
    onError: (error) => {
      toast({
        title: 'Failed to update policies',
        description: error instanceof Error ? error.message : 'Unknown error',
        variant: 'destructive',
      });
    },
  });

  const togglePolicy = (policyId: string) => {
    setSelectedPolicyIds((prev) =>
      prev.includes(policyId) ? prev.filter((id) => id !== policyId) : [...prev, policyId],
    );
  };

  const canonicalPolicyMap = useMemo(
    () =>
      CANONICAL_POLICIES.reduce<Record<string, string>>((acc, policy) => {
        acc[policy.id] = policy.name;
        return acc;
      }, {}),
    [],
  );

  if (!adapterId) {
    return <div className="border rounded-md p-4 text-sm text-muted-foreground">Select an adapter to view policies.</div>;
  }

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle>Effective Policy</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="text-sm">
            <div className="text-muted-foreground">Category</div>
            <div className="font-medium">{category ?? 'Unspecified'}</div>
          </div>
          <div className="text-sm space-y-2">
            <div className="text-muted-foreground">Applied Policies</div>
            <div className="flex flex-wrap gap-2">
              {(appliedPolicyIds.length ? appliedPolicyIds : ['none']).map((policyId) => (
                <Badge key={policyId} variant={policyId === 'none' ? 'secondary' : 'default'}>
                  {policyId === 'none' ? 'None assigned' : canonicalPolicyMap[policyId] ?? policyId}
                </Badge>
              ))}
            </div>
          </div>
          {categoryPolicy ? (
            <>
              <Separator />
              <div className="text-sm">
                <div className="text-muted-foreground mb-1">Category Policy Rules</div>
                <ul className="list-disc pl-4 space-y-1">
                  {categoryPolicy.rules?.map((rule) => (
                    <li key={`${rule.condition}-${rule.priority}`}>
                      {rule.action.toUpperCase()} when {rule.condition} (p{rule.priority})
                    </li>
                  )) ?? <li>No category rules defined.</li>}
                </ul>
              </div>
            </>
          ) : null}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Policy Assignment</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            {CANONICAL_POLICIES.map((policy) => (
              <label key={policy.id} className="flex items-start gap-2 rounded border p-3 cursor-pointer">
                <Checkbox
                  checked={selectedPolicyIds.includes(policy.id)}
                  onCheckedChange={() => togglePolicy(policy.id)}
                  aria-label={`Toggle policy ${policy.name}`}
                />
                <div>
                  <div className="font-medium">{policy.name}</div>
                  <div className="text-xs text-muted-foreground">{policy.description}</div>
                </div>
              </label>
            ))}
          </div>
          <div className="flex justify-end">
            <Button
              onClick={() => updatePolicyMutation.mutate(selectedPolicyIds)}
              disabled={updatePolicyMutation.isPending}
            >
              {updatePolicyMutation.isPending ? 'Saving...' : 'Apply Policies'}
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

