//! Adapter Registration Page
//!
//! Form for registering new adapters with semantic naming support.
//! Validates adapter name format per naming policy.
//!
//! Citation: Form patterns from ui/src/pages/Admin/StackFormModal.tsx
//! - useForm with react-hook-form
//! - Form validation with patterns

import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { useMutation, useQueryClient, useQuery } from '@tanstack/react-query';
import { z } from 'zod';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { useRBAC } from '@/hooks/security/useRBAC';
import { PermissionDenied } from '@/components/ui/permission-denied';
import { useTenant } from '@/providers/FeatureProviders';
import { apiClient } from '@/api/services';
import { toast } from 'sonner';
import { logger } from '@/utils/logger';
import { withErrorBoundary } from '@/components/WithErrorBoundary';
import {
  Box,
  ArrowLeft,
  AlertCircle,
  CheckCircle,
  Info,
  Loader2,
  Upload,
} from 'lucide-react';
import {
  registerAdapterRequestSchema,
  SupportedLanguages,
  AdapterNameUtils,
  adapterRevisionSchema,
} from '@/schemas/adapter.schema';
import { buildAdaptersListLink, buildAdapterDetailLink } from '@/utils/navLinks';

// Form schema for adapter registration
const formSchema = z.object({
  tenant: z.string().min(1, 'Workspace is required').max(50),
  domain: z.string().min(1, 'Domain is required').max(50).regex(/^[a-z0-9_-]+$/, 'Domain must contain only lowercase letters, numbers, underscores, and hyphens'),
  purpose: z.string().min(1, 'Purpose is required').max(50).regex(/^[a-z0-9_-]+$/, 'Purpose must contain only lowercase letters, numbers, underscores, and hyphens'),
  revision: adapterRevisionSchema,
  hash_b3: z.string().regex(/^b3:[a-f0-9]{64}$/, 'Hash must be in format: b3:{64 hex characters}'),
  rank: z.number().int().min(1, 'Rank must be at least 1').max(256, 'Rank must not exceed 256'),
  tier: z.enum(['persistent', 'warm', 'ephemeral']),
  languages: z.array(z.enum(SupportedLanguages)).min(1, 'At least one language required'),
  framework: z.string().optional(),
  description: z.string().max(1000).optional(),
  category: z.enum(['code', 'framework', 'codebase', 'ephemeral']),
  scope: z.enum(['global', 'tenant', 'repo', 'commit']).optional(),
  expires_at: z.string().optional(),
});

type FormData = z.infer<typeof formSchema>;

export function AdapterRegisterPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { can } = useRBAC();
  const { selectedTenant } = useTenant();
  const [selectedLanguages, setSelectedLanguages] = useState<string[]>([]);

  // Check permissions
  if (!can('AdapterRegister')) {
    return (
      <DensityProvider pageKey="adapter-register">
        <FeatureLayout
          title="Register New Adapter"
          description="Register a new LoRA adapter"
        >
          <PermissionDenied
            requiredPermission="adapter:register"
            requiredRoles={['admin', 'operator', 'developer']}
          />
        </FeatureLayout>
      </DensityProvider>
    );
  }

  const {
    register,
    handleSubmit,
    watch,
    setValue,
    formState: { errors, isSubmitting },
    reset,
  } = useForm<FormData>({
    resolver: zodResolver(formSchema),
    defaultValues: {
      tenant: selectedTenant || '',
      domain: '',
      purpose: '',
      revision: 'r001',
      hash_b3: '',
      rank: 16,
      tier: 'warm',
      languages: [],
      framework: '',
      description: '',
      category: 'code',
      scope: 'tenant',
    },
  });

  const tenant = watch('tenant');
  const domain = watch('domain');
  const purpose = watch('purpose');
  const revision = watch('revision');

  // Compute the full adapter name
  const adapterName = tenant && domain && purpose && revision
    ? `${tenant}/${domain}/${purpose}/${revision}`
    : '';

  // Validate name mutation
  const validateNameMutation = useMutation({
    mutationFn: async (name: string) => {
      return apiClient.validateAdapterName({ name });
    },
  });

  // Register adapter mutation
  const registerMutation = useMutation({
    mutationFn: async (data: FormData) => {
      const adapterId = `${data.tenant}-${data.domain}-${data.purpose}-${data.revision}`.replace(/\//g, '-');
      const name = `${data.tenant}/${data.domain}/${data.purpose}/${data.revision}`;

      return apiClient.registerAdapter({
        adapter_id: adapterId,
        name,
        hash_b3: data.hash_b3,
        rank: data.rank,
        tier: data.tier,
        languages: data.languages,
        framework: data.framework,
        category: data.category,
        scope: data.scope,
        expires_at: data.expires_at,
        metadata_json: data.description ? JSON.stringify({ description: data.description }) : undefined,
      });
    },
    onSuccess: (adapter) => {
      queryClient.invalidateQueries({ queryKey: ['adapters'] });
      toast.success(`Adapter "${adapterName}" registered successfully`);
      logger.info('Adapter registered', {
        component: 'AdapterRegisterPage',
        operation: 'registerAdapter',
        adapterId: adapter.id,
      });
      navigate(buildAdapterDetailLink(adapter.id) + '#overview', {
        state: { fromRegister: true, adapterName },
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to register adapter: ${error.message}`);
      logger.error('Failed to register adapter', {
        component: 'AdapterRegisterPage',
        operation: 'registerAdapter',
      }, error);
    },
  });

  const onSubmit = async (data: FormData) => {
    // Validate name first
    if (adapterName) {
      try {
        const validation = await validateNameMutation.mutateAsync(adapterName);
        if (!validation.valid) {
          toast.error(`Invalid adapter name: ${validation.error}`);
          return;
        }
      } catch (err) {
        // Continue if validation endpoint fails - server will validate on register
        logger.warn('Name validation skipped', { component: 'AdapterRegisterPage' });
      }
    }

    // Set languages from selected state
    data.languages = selectedLanguages as typeof data.languages;

    await registerMutation.mutateAsync(data);
  };

  const toggleLanguage = (lang: string) => {
    setSelectedLanguages(prev =>
      prev.includes(lang)
        ? prev.filter(l => l !== lang)
        : [...prev, lang]
    );
  };

  return (
    <DensityProvider pageKey="adapter-register">
      <FeatureLayout
        title="Register New Adapter"
        description="Register a new LoRA adapter with semantic naming"
        maxWidth="lg"
        contentPadding="default"
      >
        <div className="flex justify-end mb-6">
          <Button
            variant="outline"
            size="sm"
            onClick={() => navigate(buildAdaptersListLink())}
          >
            <ArrowLeft className="h-4 w-4 mr-2" />
            Back to Adapters
          </Button>
        </div>

        <form onSubmit={handleSubmit(onSubmit)} className="space-y-6">
          {/* Semantic Name Card */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Box className="h-5 w-5" />
                Semantic Naming
              </CardTitle>
              <CardDescription>
                Adapter names follow the format: tenant/domain/purpose/revision
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label htmlFor="tenant">
                    Workspace <span className="text-destructive">*</span>
                  </Label>
                  <Input
                    id="tenant"
                    placeholder="e.g., shop-floor"
                    {...register('tenant')}
                  />
                  {errors.tenant && (
                    <p className="text-sm text-destructive">{errors.tenant.message}</p>
                  )}
                </div>

                <div className="space-y-2">
                  <Label htmlFor="domain">
                    Domain <span className="text-destructive">*</span>
                  </Label>
                  <Input
                    id="domain"
                    placeholder="e.g., hydraulics"
                    {...register('domain')}
                  />
                  {errors.domain && (
                    <p className="text-sm text-destructive">{errors.domain.message}</p>
                  )}
                </div>

                <div className="space-y-2">
                  <Label htmlFor="purpose">
                    Purpose <span className="text-destructive">*</span>
                  </Label>
                  <Input
                    id="purpose"
                    placeholder="e.g., troubleshooting"
                    {...register('purpose')}
                  />
                  {errors.purpose && (
                    <p className="text-sm text-destructive">{errors.purpose.message}</p>
                  )}
                </div>

                <div className="space-y-2">
                  <Label htmlFor="revision">
                    Revision <span className="text-destructive">*</span>
                  </Label>
                  <Input
                    id="revision"
                    placeholder="r001"
                    {...register('revision')}
                  />
                  {errors.revision && (
                    <p className="text-sm text-destructive">{errors.revision.message}</p>
                  )}
                </div>
              </div>

              {/* Name Preview */}
              {adapterName && (
                <Alert>
                  <Info className="h-4 w-4" />
                  <AlertTitle>Adapter Name Preview</AlertTitle>
                  <AlertDescription className="font-mono">
                    {adapterName}
                  </AlertDescription>
                </Alert>
              )}
            </CardContent>
          </Card>

          {/* Technical Details Card */}
          <Card>
            <CardHeader>
              <CardTitle>Technical Details</CardTitle>
              <CardDescription>
                LoRA configuration and hash verification
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label htmlFor="hash_b3">
                    BLAKE3 Hash <span className="text-destructive">*</span>
                  </Label>
                  <Input
                    id="hash_b3"
                    placeholder="b3:abc123..."
                    className="font-mono text-sm"
                    {...register('hash_b3')}
                  />
                  {errors.hash_b3 && (
                    <p className="text-sm text-destructive">{errors.hash_b3.message}</p>
                  )}
                  <p className="text-xs text-muted-foreground">
                    Content-addressed hash of adapter weights
                  </p>
                </div>

                <div className="space-y-2">
                  <Label htmlFor="rank">
                    LoRA Rank <span className="text-destructive">*</span>
                  </Label>
                  <Input
                    id="rank"
                    type="number"
                    min={1}
                    max={256}
                    {...register('rank', { valueAsNumber: true })}
                  />
                  {errors.rank && (
                    <p className="text-sm text-destructive">{errors.rank.message}</p>
                  )}
                  <p className="text-xs text-muted-foreground">
                    Typical values: 8, 16, 32, 64
                  </p>
                </div>

                <div className="space-y-2">
                  <Label htmlFor="tier">
                    Storage Policy <span className="text-destructive">*</span>
                  </Label>
                  <Select
                    value={watch('tier')}
                    onValueChange={(value) => setValue('tier', value as 'persistent' | 'warm' | 'ephemeral')}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Select storage policy" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="persistent">Keep (always retained)</SelectItem>
                      <SelectItem value="warm">Standard (auto-managed)</SelectItem>
                      <SelectItem value="ephemeral">Temporary (auto-evict)</SelectItem>
                    </SelectContent>
                  </Select>
                  {errors.tier && (
                    <p className="text-sm text-destructive">{errors.tier.message}</p>
                  )}
                  <p className="text-xs text-muted-foreground">
                    Keep: never evicted, Standard: managed by lifecycle, Temporary: evicted when memory is low
                  </p>
                </div>

                <div className="space-y-2">
                  <Label htmlFor="category">
                    Category <span className="text-destructive">*</span>
                  </Label>
                  <Select
                    value={watch('category')}
                    onValueChange={(value: 'code' | 'framework' | 'codebase' | 'ephemeral') => setValue('category', value)}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Select category" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="code">Code</SelectItem>
                      <SelectItem value="framework">Framework</SelectItem>
                      <SelectItem value="codebase">Codebase</SelectItem>
                      <SelectItem value="ephemeral">Ephemeral</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>

              <Separator />

              {/* Languages */}
              <div className="space-y-2">
                <Label>
                  Supported Languages <span className="text-destructive">*</span>
                </Label>
                <div className="flex flex-wrap gap-2">
                  {SupportedLanguages.map((lang) => (
                    <Badge
                      key={lang}
                      variant={selectedLanguages.includes(lang) ? 'default' : 'outline'}
                      className="cursor-pointer"
                      onClick={() => toggleLanguage(lang)}
                    >
                      {selectedLanguages.includes(lang) && (
                        <CheckCircle className="h-3 w-3 mr-1" />
                      )}
                      {lang}
                    </Badge>
                  ))}
                </div>
                {selectedLanguages.length === 0 && (
                  <p className="text-sm text-destructive">At least one language is required</p>
                )}
              </div>

              <div className="space-y-2">
                <Label htmlFor="framework">Framework (Optional)</Label>
                <Input
                  id="framework"
                  placeholder="e.g., react, django, fastapi"
                  {...register('framework')}
                />
              </div>
            </CardContent>
          </Card>

          {/* Additional Options */}
          <Card>
            <CardHeader>
              <CardTitle>Additional Options</CardTitle>
              <CardDescription>
                Optional metadata and settings
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="description">Description</Label>
                <Textarea
                  id="description"
                  placeholder="Describe the purpose and capabilities of this adapter..."
                  rows={3}
                  {...register('description')}
                />
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label htmlFor="scope">Scope</Label>
                  <Select
                    value={watch('scope')}
                    onValueChange={(value: 'global' | 'tenant' | 'repo' | 'commit') => setValue('scope', value)}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Select scope" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="global">Global</SelectItem>
                      <SelectItem value="tenant">Workspace</SelectItem>
                      <SelectItem value="repo">Repository</SelectItem>
                      <SelectItem value="commit">Commit</SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                <div className="space-y-2">
                  <Label htmlFor="expires_at">Expiration (Optional)</Label>
                  <Input
                    id="expires_at"
                    type="datetime-local"
                    {...register('expires_at')}
                  />
                  <p className="text-xs text-muted-foreground">
                    Leave empty for permanent adapter
                  </p>
                </div>
              </div>
            </CardContent>
          </Card>

          {/* Submit */}
          <div className="flex justify-end gap-4">
            <Button
              type="button"
              variant="outline"
              onClick={() => navigate(buildAdaptersListLink())}
            >
              Cancel
            </Button>
            <Button
              type="submit"
              disabled={isSubmitting || registerMutation.isPending || selectedLanguages.length === 0}
            >
              {(isSubmitting || registerMutation.isPending) ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  Registering...
                </>
              ) : (
                <>
                  <Upload className="h-4 w-4 mr-2" />
                  Register Adapter
                </>
              )}
            </Button>
          </div>
        </form>
      </FeatureLayout>
    </DensityProvider>
  );
}

export default withErrorBoundary(AdapterRegisterPage, 'Failed to load adapter registration page');
