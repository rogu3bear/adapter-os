import { ProgressiveHint } from '@/hooks/tutorial/useProgressiveHints';

export const pageHints: Record<string, ProgressiveHint[]> = {
  adapters: [
    {
      id: 'adapter-lifecycle',
      title: 'Adapter Lifecycle',
      content: 'Adapters progress through states: registered → training → testing → production. Use the state visualization to track adapter status and memory usage.',
      trigger: 'first-visit',
      placement: 'top'
    },
    {
      id: 'empty-adapters',
      title: 'No Adapters Yet',
      content: 'Start by training your first adapter. Click "Train New Adapter" to create a specialized adapter for your use case.',
      trigger: 'empty-state',
      placement: 'top'
    }
  ],
  inference: [
    {
      id: 'inference-workflow',
      title: 'Inference Playground',
      content: 'Select an adapter (or use the base model), enter your prompt, and adjust parameters. Use comparison mode to test multiple configurations side-by-side.',
      trigger: 'first-visit',
      placement: 'top'
    },
    {
      id: 'no-adapters-inference',
      title: 'No Adapters Available',
      content: 'Train an adapter first, or run inference with the base model only. Check the Adapters page to see available adapters.',
      trigger: 'empty-state',
      placement: 'top'
    }
  ],
  policies: [
    {
      id: 'policy-packs',
      title: 'Policy Packs Overview',
      content: 'AdapterOS enforces 20 policy packs covering security, determinism, and compliance. Each pack contains multiple rules. Use the tabs to view packs, compliance status, and audit trails.',
      trigger: 'first-visit',
      placement: 'top'
    },
    {
      id: 'create-policy',
      title: 'Create Policy Pack',
      content: 'Click "New Policy" to create a custom policy pack. Configure rules for your tenant\'s specific security and compliance requirements.',
      trigger: 'custom',
      placement: 'top'
    }
  ],
  training: [
    {
      id: 'training-setup',
      title: 'Training Setup',
      content: 'Configure your training job: select data source, set hyperparameters, and choose adapter category. The wizard guides you through each step.',
      trigger: 'first-visit',
      placement: 'top'
    },
    {
      id: 'data-source',
      title: 'Data Source Selection',
      content: 'Choose between repository-based training (uses code repository), directory-based (local files), or custom dataset path for fine-tuning.',
      trigger: 'custom',
      placement: 'bottom'
    }
  ],
  routing: [
    {
      id: 'router-config',
      title: 'Router Configuration',
      content: 'Configure K-sparse routing with Q15 quantized gates. The router selects the top K adapters based on context similarity and performance metrics.',
      trigger: 'first-visit',
      placement: 'top'
    },
    {
      id: 'gate-thresholds',
      title: 'Gate Thresholds',
      content: 'Adjust gate thresholds to control adapter selection. Lower thresholds include more adapters, higher thresholds are more selective.',
      trigger: 'custom',
      placement: 'bottom'
    }
  ]
};

export function getPageHints(pageKey: string): ProgressiveHint[] {
  return pageHints[pageKey] ?? [];
}

