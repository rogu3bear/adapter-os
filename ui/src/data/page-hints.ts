import { ProgressiveHint } from '../hooks/useProgressiveHints';

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
      title: 'Welcome to Inference!',
      content: 'Try your first inference with the default settings. The router will automatically select the best adapters for your prompt.',
      trigger: 'first-visit',
      placement: 'top'
    },
    {
      id: 'no-adapters-inference',
      title: 'No Adapters Available',
      content: 'Train an adapter first, or run inference with the base model only. Check the Adapters page to see available adapters.',
      trigger: 'empty-state',
      placement: 'top'
    },
    {
      id: 'auto-select-adapter',
      title: 'Smart Adapter Selection',
      content: '🤖 "Auto-select" uses the router to automatically choose optimal adapters based on your prompt content. This usually gives the best results!',
      condition: () => true, // Always show when adapters are available
      placement: 'bottom-left',
      trigger: 'user-action'
    },
    {
      id: 'streaming-mode',
      title: 'Real-time Streaming',
      content: 'Enable streaming mode to see tokens appear in real-time as they\'re generated. Perfect for longer responses and interactive experiences.',
      condition: () => true,
      placement: 'top-right',
      trigger: 'user-action'
    },
    {
      id: 'comparison-mode-intro',
      title: 'A/B Testing Made Easy',
      content: 'Use comparison mode to test different adapters, temperatures, or parameters side-by-side. See performance metrics and choose the best configuration.',
      condition: () => true,
      placement: 'top-center',
      trigger: 'user-action'
    },
    {
      id: 'batch-inference',
      title: 'Process Multiple Prompts',
      content: 'Batch mode lets you process multiple prompts simultaneously with shared configuration. Great for evaluating responses across different inputs.',
      condition: () => true,
      placement: 'bottom-right',
      trigger: 'user-action'
    },
    {
      id: 'advanced-parameters',
      title: 'Fine-tune Generation',
      content: 'Temperature controls creativity (0.1-0.3 for focused, 0.7-1.0 for creative). Top-k and Top-p help control response diversity.',
      condition: () => true,
      placement: 'bottom-left',
      trigger: 'user-action'
    },
    {
      id: 'session-history',
      title: 'Learn from History',
      content: 'Your previous inferences are saved locally. Click on any session to reload the prompt and continue iterating.',
      condition: () => true,
      placement: 'bottom-center',
      trigger: 'delayed'
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

