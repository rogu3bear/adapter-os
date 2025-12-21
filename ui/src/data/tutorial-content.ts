import type { TutorialConfig } from '@/components/ContextualTutorial';
import tutorialsJson from '@/../../shared/tutorials.json';

// Type for the shared JSON format
interface SharedTutorialStep {
  id: string;
  title: string;
  content: string;
  target_selector: string | null;
  position: string;
}

interface SharedTutorial {
  id: string;
  title: string;
  description: string;
  dismissible: boolean;
  steps: SharedTutorialStep[];
}

// Convert shared JSON format to TutorialConfig
function convertToTutorialConfig(shared: SharedTutorial): TutorialConfig {
  return {
    id: shared.id,
    title: shared.title,
    description: shared.description,
    dismissible: shared.dismissible,
    steps: shared.steps.map(step => ({
      id: step.id,
      title: step.title,
      content: step.content,
      targetSelector: step.target_selector ?? undefined,
      position: step.position as 'top' | 'bottom' | 'left' | 'right' | 'center'
    }))
  };
}

// Load tutorials from shared JSON
const sharedTutorials = tutorialsJson.tutorials as SharedTutorial[];

// Tutorial registry built from shared JSON
export const tutorialRegistry: Record<string, TutorialConfig> = Object.fromEntries(
  sharedTutorials.map(t => [t.id, convertToTutorialConfig(t)])
);

// Export individual tutorials for backward compatibility
export const trainingTutorial = tutorialRegistry['training-tutorial'];
export const adapterManagementTutorial = tutorialRegistry['adapter-management-tutorial'];
export const policyManagementTutorial = tutorialRegistry['policy-management-tutorial'];
export const dashboardTutorial = tutorialRegistry['dashboard-tutorial'];

export function getTutorial(id: string): TutorialConfig | undefined {
  return tutorialRegistry[id];
}

export function getTutorialsForPage(pagePath: string): TutorialConfig[] {
  const pageTutorials: Record<string, string[]> = {
    '/training': ['training-tutorial'],
    '/adapters': ['adapter-management-tutorial'],
    '/security/policies': ['policy-management-tutorial'],
    '/dashboard': ['dashboard-tutorial']
  };

  const tutorialIds = pageTutorials[pagePath] || [];
  return tutorialIds.map(id => tutorialRegistry[id]).filter(Boolean) as TutorialConfig[];
}

