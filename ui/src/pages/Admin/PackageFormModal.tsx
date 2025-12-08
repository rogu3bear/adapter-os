import { useEffect, useMemo, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import type { Adapter, AdapterPackage, AdapterStrengthSetting, CreatePackageRequest, UpdatePackageRequest } from '@/api/types';
import { useCreatePackage, useUpdatePackage } from '@/hooks/useAdmin';
import { useQuery } from '@tanstack/react-query';
import apiClient from '@/api/client';
import { Checkbox } from '@/components/ui/checkbox';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Alert, AlertDescription } from '@/components/ui/alert';

interface PackageFormModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  pkg?: AdapterPackage;
}

const determinismOptions = ['strict', 'besteffort', 'relaxed'];
const routingOptions = ['deterministic', 'adaptive'];

export function PackageFormModal({ open, onOpenChange, pkg }: PackageFormModalProps) {
  const isEdit = !!pkg;
  const createPackage = useCreatePackage();
  const updatePackage = useUpdatePackage();

  const { data: adapters } = useQuery({
    queryKey: ['adapters'],
    queryFn: () => apiClient.listAdapters(),
    enabled: open,
  });

  const { data: stacks } = useQuery({
    queryKey: ['adapter-stacks'],
    queryFn: () => apiClient.listAdapterStacks(),
    enabled: open,
  });

  const [name, setName] = useState(pkg?.name || '');
  const [description, setDescription] = useState(pkg?.description || '');
  const [tagsInput, setTagsInput] = useState((pkg?.tags || []).join(', '));
  const [domain, setDomain] = useState(pkg?.domain || '');
  const [scopePath, setScopePath] = useState(pkg?.scope_path_prefix || pkg?.scope_path || '');
  const [determinismMode, setDeterminismMode] = useState(pkg?.determinism_mode || '');
  const [routingDetMode, setRoutingDetMode] = useState(pkg?.routing_determinism_mode || '');
  const [useExistingStack, setUseExistingStack] = useState<boolean>(!!pkg?.stack_id);
  const [stackId, setStackId] = useState<string>(pkg?.stack_id || '');
  const [selectedAdapters, setSelectedAdapters] = useState<AdapterStrengthSetting[]>(pkg?.adapter_strengths || []);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (pkg) {
      setName(pkg.name);
      setDescription(pkg.description || '');
      setTagsInput((pkg.tags || []).join(', '));
      setDomain(pkg.domain || '');
      setScopePath(pkg.scope_path_prefix || pkg.scope_path || '');
      setDeterminismMode(pkg.determinism_mode || '');
      setRoutingDetMode(pkg.routing_determinism_mode || '');
      setUseExistingStack(true);
      setStackId(pkg.stack_id);
      setSelectedAdapters(pkg.adapter_strengths || []);
    } else if (!open) {
      setName('');
      setDescription('');
      setTagsInput('');
      setDomain('');
      setScopePath('');
      setDeterminismMode('');
      setRoutingDetMode('');
      setUseExistingStack(false);
      setStackId('');
      setSelectedAdapters([]);
      setError(null);
    }
  }, [pkg, open]);

  const tags = useMemo(
    () =>
      tagsInput
        .split(',')
        .map((t) => t.trim())
        .filter(Boolean),
    [tagsInput]
  );

  const toggleAdapter = (adapter: Adapter) => {
    const exists = selectedAdapters.find((a) => a.adapter_id === adapter.id);
    if (exists) {
      setSelectedAdapters(selectedAdapters.filter((a) => a.adapter_id !== adapter.id));
    } else {
      setSelectedAdapters([...selectedAdapters, { adapter_id: adapter.id, strength: adapter.lora_strength ?? 1 }]);
    }
  };

  const updateStrength = (adapterId: string, strength?: number) => {
    setSelectedAdapters((prev) =>
      prev.map((a) => (a.adapter_id === adapterId ? { ...a, strength } : a))
    );
  };

  const handleSubmit = async () => {
    setError(null);
    if (!name.trim()) {
      setError('Name is required');
      return;
    }

    const payloadBase: CreatePackageRequest | UpdatePackageRequest = {
      name: name.trim(),
      description: description.trim() || undefined,
      tags,
      determinism_mode: determinismMode || undefined,
      routing_determinism_mode: routingDetMode || undefined,
      domain: domain.trim() || undefined,
      scope_path: scopePath.trim() || undefined,
      scope_path_prefix: scopePath.trim() || undefined,
    };

    const payload: CreatePackageRequest | UpdatePackageRequest = useExistingStack
      ? {
          ...payloadBase,
          stack_id: stackId || pkg?.stack_id,
          adapters: [],
        }
      : {
          ...payloadBase,
          adapters: selectedAdapters,
        };

    if (!useExistingStack && selectedAdapters.length === 0) {
      setError('Select at least one adapter or choose an existing stack');
      return;
    }

    if (useExistingStack && !(stackId || pkg?.stack_id)) {
      setError('Choose a stack or build from adapters');
      return;
    }

    try {
      if (isEdit) {
        await updatePackage.mutateAsync({ packageId: pkg!.id, data: payload as UpdatePackageRequest });
      } else {
        await createPackage.mutateAsync(payload as CreatePackageRequest);
      }
      onOpenChange(false);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const renderAdapterSelector = () => {
    if (!adapters?.length) {
      return <div className="text-sm text-muted-foreground">No adapters available.</div>;
    }

    return (
      <div className="space-y-2 max-h-64 overflow-y-auto border rounded p-2">
        {adapters.map((adapter) => {
          const selected = selectedAdapters.find((a) => a.adapter_id === adapter.id);
          return (
            <div key={adapter.id} className="flex items-center justify-between gap-3 text-sm">
              <div className="flex items-center gap-2">
                <Checkbox
                  checked={!!selected}
                  onCheckedChange={() => toggleAdapter(adapter)}
                  id={`adapter-${adapter.id}`}
                />
                <Label htmlFor={`adapter-${adapter.id}`} className="cursor-pointer">
                  {adapter.name || adapter.id}
                </Label>
              </div>
              {selected && (
                <Input
                  type="number"
                  step="0.1"
                  min="0"
                  max="2"
                  className="w-24"
                  value={selected.strength ?? 1}
                  onChange={(e) => updateStrength(adapter.id, Number(e.target.value))}
                />
              )}
            </div>
          );
        })}
      </div>
    );
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{isEdit ? 'Edit Package' : 'Create Package'}</DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          <div className="space-y-2">
            <Label>Name</Label>
            <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="team.pkg.alpha" />
          </div>

          <div className="space-y-2">
            <Label>Description</Label>
            <Textarea value={description} onChange={(e) => setDescription(e.target.value)} />
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-2">
              <Label>Tags (comma separated)</Label>
              <Input value={tagsInput} onChange={(e) => setTagsInput(e.target.value)} placeholder="beta,internal" />
            </div>
            <div className="space-y-2">
              <Label>Domain (optional)</Label>
              <Input value={domain} onChange={(e) => setDomain(e.target.value)} placeholder="code, docs, etc." />
            </div>
            <div className="space-y-2">
              <Label>Scope path prefix (optional)</Label>
              <Input value={scopePath} onChange={(e) => setScopePath(e.target.value)} placeholder="repo/path" />
            </div>
            <div className="space-y-2">
              <Label>Determinism mode</Label>
              <Select value={determinismMode || 'inherit'} onValueChange={(v) => setDeterminismMode(v === 'inherit' ? '' : v)}>
                <SelectTrigger>
                  <SelectValue placeholder="inherit tenant default" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="inherit">Inherit</SelectItem>
                  {determinismOptions.map((opt) => (
                    <SelectItem key={opt} value={opt}>{opt}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label>Routing determinism</Label>
              <Select value={routingDetMode || 'inherit'} onValueChange={(v) => setRoutingDetMode(v === 'inherit' ? '' : v)}>
                <SelectTrigger>
                  <SelectValue placeholder="inherit" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="inherit">Inherit</SelectItem>
                  {routingOptions.map((opt) => (
                    <SelectItem key={opt} value={opt}>{opt}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>

          <div className="space-y-2">
            <Label>Bind to existing stack?</Label>
            <div className="flex items-center gap-2">
              <Checkbox checked={useExistingStack} onCheckedChange={(checked) => setUseExistingStack(!!checked)} id="use-existing-stack" />
              <Label htmlFor="use-existing-stack" className="cursor-pointer">Use existing stack</Label>
            </div>
            {useExistingStack ? (
              <Select value={stackId} onValueChange={setStackId}>
                <SelectTrigger>
                  <SelectValue placeholder="Select a stack" />
                </SelectTrigger>
                <SelectContent>
                  {(stacks || []).map((stack) => (
                    <SelectItem key={stack.id} value={stack.id}>
                      {stack.name} ({stack.adapter_ids?.length || 0} adapters)
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <div className="space-y-2">
                <Label>Select adapters and strengths</Label>
                {renderAdapterSelector()}
              </div>
            )}
          </div>

          {error && (
            <Alert variant="destructive">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
        </div>

        <DialogFooter className="gap-2">
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={handleSubmit} disabled={createPackage.isPending || updatePackage.isPending}>
            {isEdit ? 'Save changes' : 'Create package'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export default PackageFormModal;

