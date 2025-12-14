import React, { useMemo, useState, useEffect } from 'react';
import { useLocation, useNavigate, useParams, Link } from 'react-router-dom';
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
import { useAdapterDetail } from '@/hooks/adapters';
import { useAdapterTabRouter } from '@/hooks/navigation/useTabRouter';
import type { AdapterCategory, AdapterDetailResponse } from '@/api/adapter-types';
import { isAdapterCategory } from '@/utils/typeGuards';
import { CANONICAL_POLICIES } from '@/api/policyTypes';
import apiClient from '@/api/client';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Checkbox } from '@/components/ui/checkbox';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { useToast } from '@/hooks/use-toast';
import { Separator } from '@/components/ui/separator';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Loader2 } from 'lucide-react';
import { DetailPageLoadingState } from '@/components/ui/loading-patterns';

export default function AdaptersShell() {
  const { adapterId } = useParams<{ adapterId: string }>();
  const location = useLocation();
  const navigate = useNavigate();
  const [postRegisterBanner, setPostRegisterBanner] = useState<{ adapterName?: string } | null>(null);

  const { activeTab, setActiveTab, availableTabs, getTabPath } = useAdapterTabRouter();

  const {
    adapter: adapterDetail,
    lineage,
    activations,
    manifest,
    isLoadingDetail,
    isLoadingLineage,
    isLoadingActivations,
    isLoadingManifest,
    refetchActivations,
    refetchLineage,
    refetchManifest,
  } = useAdapterDetail(adapterId || '', { enabled: !!adapterId });

  // Check if any tab is loading
  const isAnyTabLoading = isLoadingDetail || isLoadingLineage || isLoadingActivations || isLoadingManifest;

  useEffect(() => {
    const state = (location.state || {}) as { fromRegister?: boolean; adapterName?: string };
    if (state.fromRegister) {
      setPostRegisterBanner({ adapterName: state.adapterName });
      navigate(location.pathname, { replace: true, state: {} });
    }
  }, [location.pathname, location.state, navigate]);

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
      <Tabs value={activeTab} onValueChange={(value) => setActiveTab(value as typeof activeTab)}>
        <TabsList className="w-full grid grid-cols-3 md:grid-cols-4 lg:grid-cols-7">
          {availableTabs.map(tab => {
            const isTabLoading =
              (tab.id === 'overview' && isLoadingDetail) ||
              (tab.id === 'lineage' && isLoadingLineage) ||
              (tab.id === 'activations' && isLoadingActivations) ||
              (tab.id === 'manifest' && isLoadingManifest);

            return (
              <TabsTrigger key={tab.id} value={tab.id} disabled={isTabLoading} asChild>
                <Link to={getTabPath(tab.id)}>
                  {tab.label}
                  {tab.id === 'overview' && isLoadingDetail && <Loader2 className="ml-1 h-3 w-3 animate-spin" />}
                  {tab.id === 'activations' && isLoadingActivations && <Loader2 className="ml-1 h-3 w-3 animate-spin" />}
                  {tab.id === 'lineage' && isLoadingLineage && <Loader2 className="ml-1 h-3 w-3 animate-spin" />}
                  {tab.id === 'manifest' && isLoadingManifest && <Loader2 className="ml-1 h-3 w-3 animate-spin" />}
                </Link>
              </TabsTrigger>
            );
          })}
        </TabsList>

        <TabsContent value="list" className="mt-6">
          <AdaptersList />
        </TabsContent>
        <TabsContent value="overview" className="mt-6">
          {isLoadingDetail ? (
            <DetailPageLoadingState />
          ) : adapterId ? (
            <AdapterDetailPage />
          ) : (
            <AdaptersList />
          )}
        </TabsContent>
        <TabsContent value="activations" className="mt-6">
          {isLoadingActivations ? (
            <DetailPageLoadingState />
          ) : adapterId ? (
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
          {isLoadingLineage ? (
            <DetailPageLoadingState />
          ) : adapterId ? (
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
          {isLoadingManifest ? (
            <DetailPageLoadingState />
          ) : adapterId ? (
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

  const adapterCategory = adapterDetail?.adapter?.category;
  const category = isAdapterCategory(adapterCategory) ? adapterCategory : undefined;

  const adapterWithPolicies = adapterDetail?.adapter as { policy_ids?: string[] } | undefined;
  const appliedPolicyIds = adapterWithPolicies?.policy_ids ?? [];

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

