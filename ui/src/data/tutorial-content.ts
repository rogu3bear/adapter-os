import type { TutorialConfig, TutorialStep } from '@/components/ContextualTutorial';

// Training Tutorial
export const trainingTutorial: TutorialConfig = {
  id: 'training-tutorial',
  title: 'Training Adapters Tutorial',
  description: 'Learn how to train adapters step by step',
  dismissible: true,
  steps: [
    {
      id: 'intro',
      title: 'Welcome to Training',
      content: 'This tutorial will guide you through training an adapter. Training creates specialized AI models for your domain.',
      position: 'center'
    },
    {
      id: 'select-template',
      title: 'Choose a Template',
      content: 'Start by selecting a training template. Templates provide pre-configured settings for common use cases.',
      targetSelector: '[data-tutorial="training-template"]',
      position: 'bottom'
    },
    {
      id: 'configure-params',
      title: 'Configure Parameters',
      content: 'Adjust training parameters like learning rate, batch size, and epochs to match your requirements.',
      targetSelector: '[data-tutorial="training-params"]',
      position: 'right'
    },
    {
      id: 'start-training',
      title: 'Start Training',
      content: 'Click the Start Training button to launch your training job. Monitor progress in real-time.',
      targetSelector: '[data-tutorial="start-training"]',
      position: 'top'
    }
  ]
};

// Adapter Management Tutorial
export const adapterManagementTutorial: TutorialConfig = {
  id: 'adapter-management-tutorial',
  title: 'Managing Adapters',
  description: 'Learn how to deploy and manage adapters',
  dismissible: true,
  steps: [
    {
      id: 'intro',
      title: 'Adapter Management',
      content: 'Adapters are specialized AI models trained for specific domains. Learn how to deploy and manage them.',
      position: 'center'
    },
    {
      id: 'deploy-adapter',
      title: 'Deploy an Adapter',
      content: 'Click Deploy to make a trained adapter available for inference. Ensure it has passed all tests first.',
      targetSelector: '[data-tutorial="deploy-adapter"]',
      position: 'bottom'
    },
    {
      id: 'monitor-health',
      title: 'Monitor Health',
      content: 'Check adapter health metrics regularly. Look for latency, error rates, and resource usage.',
      targetSelector: '[data-tutorial="adapter-health"]',
      position: 'right'
    }
  ]
};

// Policy Management Tutorial
export const policyManagementTutorial: TutorialConfig = {
  id: 'policy-management-tutorial',
  title: 'Policy Management',
  description: 'Learn how to manage policies and ensure compliance',
  dismissible: true,
  steps: [
    {
      id: 'intro',
      title: 'Policy Management',
      content: 'Policies enforce security and compliance rules. This tutorial shows you how to review and manage them.',
      position: 'center'
    },
    {
      id: 'review-policies',
      title: 'Review Policy Packs',
      content: 'All 20 policy packs should be reviewed regularly. Click on a policy to view its details.',
      targetSelector: '[data-tutorial="policy-list"]',
      position: 'bottom'
    },
    {
      id: 'sign-policies',
      title: 'Sign Policies',
      content: 'After reviewing, sign policies to indicate compliance. Unsigned policies may block operations.',
      targetSelector: '[data-tutorial="sign-policy"]',
      position: 'right'
    }
  ]
};

// Dashboard Tutorial
export const dashboardTutorial: TutorialConfig = {
  id: 'dashboard-tutorial',
  title: 'Dashboard Overview',
  description: 'Learn how to navigate and use the dashboard',
  dismissible: true,
  steps: [
    {
      id: 'intro',
      title: 'Welcome to the Dashboard',
      content: 'The dashboard provides a system overview with health metrics, adapter counts, and performance indicators.',
      position: 'center'
    },
    {
      id: 'system-health',
      title: 'System Health',
      content: 'Monitor system health metrics here. Green indicates healthy, yellow means attention needed, red requires immediate action.',
      targetSelector: '[data-tutorial="system-health"]',
      position: 'bottom'
    },
    {
      id: 'recent-activity',
      title: 'Recent Activity',
      content: 'View recent system events and activities. This helps you stay informed about what\'s happening.',
      targetSelector: '[data-tutorial="recent-activity"]',
      position: 'right'
    }
  ]
};

// Tutorial registry
export const tutorialRegistry: Record<string, TutorialConfig> = {
  'training-tutorial': trainingTutorial,
  'adapter-management-tutorial': adapterManagementTutorial,
  'policy-management-tutorial': policyManagementTutorial,
  'dashboard-tutorial': dashboardTutorial
};

export function getTutorial(id: string): TutorialConfig | undefined {
  return tutorialRegistry[id];
}

export function getTutorialsForPage(pagePath: string): TutorialConfig[] {
  const pageTutorials: Record<string, string[]> = {
    '/training': ['training-tutorial'],
    '/adapters': ['adapter-management-tutorial'],
    '/policies': ['policy-management-tutorial'],
    '/dashboard': ['dashboard-tutorial']
  };

  const tutorialIds = pageTutorials[pagePath] || [];
  return tutorialIds.map(id => tutorialRegistry[id]).filter(Boolean) as TutorialConfig[];
}

