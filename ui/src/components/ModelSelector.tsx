import React from 'react';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import apiClient from '../api/client';
import type { OpenAIModelInfo } from '../api/types';

interface ModelSelectorProps {
  value?: string;
  onChange?: (modelId: string) => void;
  disabled?: boolean;
}

export function ModelSelector({ value, onChange, disabled }: ModelSelectorProps) {
  const [models, setModels] = React.useState<OpenAIModelInfo[]>([]);
  const [loading, setLoading] = React.useState(true);

  React.useEffect(() => {
    let mounted = true;
    (async () => {
      try {
        const list = await apiClient.listModels();
        if (mounted) setModels(list);
      } catch (_) {
        // silently ignore; UI can operate without models list
      } finally {
        if (mounted) setLoading(false);
      }
    })();
    return () => { mounted = false; };
  }, []);

  const handleChange = (val: string) => {
    onChange?.(val);
  };

  return (
    <Select value={value} onValueChange={handleChange} disabled={disabled || loading}>
      <SelectTrigger className="w-[280px]">
        <SelectValue placeholder={loading ? 'Loading models…' : 'Select model'} />
      </SelectTrigger>
      <SelectContent>
        {models.map((m) => (
          <SelectItem key={m.id} value={m.id}>{m.id}</SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

