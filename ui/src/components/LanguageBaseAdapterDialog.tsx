import React, { useEffect, useMemo, useState } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';
import { Label } from './ui/label';
import { Input } from './ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Switch } from './ui/switch';
import { Button } from './ui/button';
import { Alert, AlertDescription } from './ui/alert';
import { AlertTriangle, Brain } from 'lucide-react';
import { errorRecoveryTemplates } from './ui/error-recovery';
import apiClient from '../api/client';
import { StartTrainingRequest, TrainingConfig } from '../api/types';

interface LanguageBaseAdapterDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selectedTenant?: string;
  onSuccess?: (jobId: string) => void;
}

const SUPPORTED_LANGUAGES = ['Rust', 'Python', 'TypeScript', 'JavaScript', 'Go'] as const;
type SupportedLanguage = typeof SUPPORTED_LANGUAGES[number];

export function LanguageBaseAdapterDialog({
  open,
  onOpenChange,
  selectedTenant,
  onSuccess,
}: LanguageBaseAdapterDialogProps) {
  const [language, setLanguage] = useState<SupportedLanguage | ''>('');
  const [adapterName, setAdapterName] = useState('');
  const [directoryRoot, setDirectoryRoot] = useState('');
  const [directoryPath, setDirectoryPath] = useState('.');
  const [tenantId, setTenantId] = useState<string>(selectedTenant || '');
  const [rank, setRank] = useState<number>(16);
  const [alpha, setAlpha] = useState<number>(32);
  const [epochs, setEpochs] = useState<number>(3);
  const [learningRate, setLearningRate] = useState<number>(0.001);
  const [batchSize, setBatchSize] = useState<number>(32);
  const [packaging, setPackaging] = useState<boolean>(false);
  const [adaptersRoot, setAdaptersRoot] = useState<string>('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [statusMessage, setStatusMessage] = useState<{ message: string; variant: 'success' | 'info' | 'warning' } | null>(null);
  const [errorRecovery, setErrorRecovery] = useState<React.ReactElement | null>(null);

  const showStatus = (message: string, variant: 'success' | 'info' | 'warning') => {
    setStatusMessage({ message, variant });
  };

  useEffect(() => {
    if (open) {
      // Reset form on open for a fresh start
      setLanguage('');
      setAdapterName('');
      setDirectoryRoot('');
      setDirectoryPath('.');
      setTenantId(selectedTenant || '');
      setRank(16);
      setAlpha(32);
      setEpochs(3);
      setLearningRate(0.001);
      setBatchSize(32);
      setPackaging(false);
      setAdaptersRoot('');
      setIsSubmitting(false);
      setError(null);
      setStatusMessage(null);
      setErrorRecovery(null);
    }
  }, [open, selectedTenant]);

  const languageValid = useMemo(() => SUPPORTED_LANGUAGES.includes(language as any), [language]);

  const isAbsolutePath = (p: string) => {
    if (!p) return false;
    // POSIX absolute (/path), Windows drive letter (C:\ or C:/), or UNC (\\server\share)
    return p.startsWith('/') || /^[A-Za-z]:[\\\/]/.test(p) || /^\\\\/.test(p);
  };

  const formValid = useMemo(() => {
    if (!languageValid) return false;
    if (!adapterName.trim()) return false;
    if (!isAbsolutePath(directoryRoot)) return false;
    if (packaging && !adaptersRoot.trim()) return false;
    return true;
  }, [languageValid, adapterName, directoryRoot, packaging, adaptersRoot]);

  const handleSubmit = async () => {
    if (!formValid) {
      setError('Please fix validation errors before submitting.');
      return;
    }

    setIsSubmitting(true);
    setError(null);
    try {
      const config: TrainingConfig = {
        rank,
        alpha,
        targets: ['q_proj', 'v_proj'],
        epochs,
        learning_rate: learningRate,
        batch_size: batchSize,
      };

      // Compose request. Note: `language` is currently a UI-only field unless server expects it.
      const req: StartTrainingRequest = {
        adapter_name: adapterName,
        config,
        directory_root: directoryRoot,
        directory_path: directoryPath || '.',
        tenant_id: tenantId || undefined,
        package: packaging || undefined,
        adapters_root: packaging ? adaptersRoot : undefined,
        category: 'code',
        language: language,
      };

      const job = await apiClient.startTraining(req);
      showStatus(`Training job ${job.id} started.`, 'success');
      onSuccess?.(job.id);
      onOpenChange(false);
    } catch (e: any) {
      const message = e?.message || 'Failed to start training';
      setError(message);
      setStatusMessage({ message, variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          e instanceof Error ? e : new Error(message),
          () => handleSubmit()
        )
      );
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[720px] max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Brain className="h-5 w-5" />
            Train Language Base Adapter
          </DialogTitle>
        </DialogHeader>

        {errorRecovery && (
          <div className="mb-4">
            {errorRecovery}
          </div>
        )}

        {statusMessage && (
          <Alert
            className={
              statusMessage.variant === 'success'
                ? 'border-green-200 bg-green-50'
                : statusMessage.variant === 'warning'
                  ? 'border-amber-200 bg-amber-50'
                  : 'border-blue-200 bg-blue-50'
            }
          >
            <AlertDescription
              className={
                statusMessage.variant === 'success'
                  ? 'text-green-700'
                  : statusMessage.variant === 'warning'
                    ? 'text-amber-700'
                    : 'text-blue-700'
              }
            >
              {statusMessage.message}
            </AlertDescription>
          </Alert>
        )}

        <div className="space-y-6 py-2">
          {error && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="language">Language</Label>
              <Select value={language} onValueChange={(v) => setLanguage(v as SupportedLanguage)}>
                <SelectTrigger id="language">
                  <SelectValue placeholder="Select language" />
                </SelectTrigger>
                <SelectContent>
                  {SUPPORTED_LANGUAGES.map((lang) => (
                    <SelectItem key={lang} value={lang}>{lang}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {!languageValid && (
                <p className="text-xs text-muted-foreground">Choose a supported language: Rust, Python, TypeScript, JavaScript, Go.</p>
              )}
            </div>

            <div className="space-y-2">
              <Label htmlFor="adapterName">Adapter name</Label>
              <Input id="adapterName" placeholder="rust-base-v1" value={adapterName} onChange={(e) => setAdapterName(e.target.value)} />
            </div>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="directoryRoot">directory_root (absolute)</Label>
              <Input id="directoryRoot" placeholder="/absolute/path/to/code" value={directoryRoot} onChange={(e) => setDirectoryRoot(e.target.value)} />
              {!isAbsolutePath(directoryRoot) && (
                <p className="text-xs text-muted-foreground">Must be an absolute path (e.g., /Users/me/project or C:\\repo)</p>
              )}
            </div>
            <div className="space-y-2">
              <Label htmlFor="directoryPath">directory_path (relative)</Label>
              <Input id="directoryPath" placeholder="." value={directoryPath} onChange={(e) => setDirectoryPath(e.target.value)} />
              <p className="text-xs text-muted-foreground">Relative to directory_root. Defaults to "."</p>
            </div>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="space-y-2">
              <Label htmlFor="rank">Rank</Label>
              <Input id="rank" type="number" value={rank} onChange={(e) => setRank(parseInt(e.target.value) || 16)} />
            </div>
            <div className="space-y-2">
              <Label htmlFor="alpha">Alpha</Label>
              <Input id="alpha" type="number" value={alpha} onChange={(e) => setAlpha(parseInt(e.target.value) || 32)} />
            </div>
            <div className="space-y-2">
              <Label htmlFor="epochs">Epochs</Label>
              <Input id="epochs" type="number" value={epochs} onChange={(e) => setEpochs(parseInt(e.target.value) || 3)} />
            </div>
            <div className="space-y-2">
              <Label htmlFor="learningRate">Learning rate</Label>
              <Input id="learningRate" type="number" step="0.0001" value={learningRate} onChange={(e) => setLearningRate(parseFloat(e.target.value) || 0.001)} />
            </div>
            <div className="space-y-2">
              <Label htmlFor="batchSize">Batch size</Label>
              <Input id="batchSize" type="number" value={batchSize} onChange={(e) => setBatchSize(parseInt(e.target.value) || 32)} />
            </div>
          </div>

          <div className="space-y-2">
            <Label htmlFor="tenantId">Tenant ID (optional)</Label>
            <Input id="tenantId" placeholder="default" value={tenantId} onChange={(e) => setTenantId(e.target.value)} />
          </div>

          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <Label htmlFor="packageToggle">Package after training</Label>
              <Switch id="packageToggle" checked={packaging} onCheckedChange={(v) => setPackaging(!!v)} />
            </div>
            {packaging && (
              <div className="space-y-2">
                <Label htmlFor="adaptersRoot">adapters_root</Label>
                <Input id="adaptersRoot" placeholder="./adapters" value={adaptersRoot} onChange={(e) => setAdaptersRoot(e.target.value)} />
                {!adaptersRoot && (
                  <p className="text-xs text-muted-foreground">Required if packaging is enabled.</p>
                )}
              </div>
            )}
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={isSubmitting}>Cancel</Button>
          <Button onClick={handleSubmit} disabled={isSubmitting || !formValid}>{isSubmitting ? 'Starting…' : 'Start Training'}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export default LanguageBaseAdapterDialog;
