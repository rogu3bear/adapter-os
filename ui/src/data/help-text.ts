export interface HelpTextItem {
  id: string;
  title: string;
  content: string;
  category: 'navigation' | 'operations' | 'adapters' | 'policies' | 'settings' | 'technical';
}

export const helpTextDatabase: HelpTextItem[] = [
  // Navigation Help
  {
    id: 'dashboard',
    title: 'Dashboard',
    content: 'System overview showing health metrics, adapter counts, and performance indicators. Monitor system status and alerts.',
    category: 'navigation'
  },
  {
    id: 'adapters',
    title: 'Adapters',
    content: 'Manage LoRA adapters for specialized AI capabilities. Create, train, and deploy adapters for specific domains.',
    category: 'navigation'
  },
  {
    id: 'policies',
    title: 'Policies',
    content: 'Configure security and compliance policies. Manage the 20 policy packs that enforce system behavior.',
    category: 'navigation'
  },
  {
    id: 'operations',
    title: 'Operations',
    content: 'Runtime management, plan execution, and system monitoring. Execute plans, manage telemetry, and monitor alerts.',
    category: 'navigation'
  },
  {
    id: 'settings',
    title: 'Settings',
    content: 'System configuration and administration. Manage tenants, nodes, and system-wide settings.',
    category: 'navigation'
  },

  // Operations Help
  {
    id: 'plans',
    title: 'Plans',
    content: 'Execution plan compilation. Plans define how adapters are loaded and executed for specific tasks.',
    category: 'operations'
  },
  {
    id: 'promotion',
    title: 'Promotion',
    content: 'Control plane promotion gates. Advanced feature for promoting control plane versions with policy compliance checks.',
    category: 'operations'
  },
  {
    id: 'telemetry',
    title: 'Telemetry',
    content: 'Event bundle management. Monitor system events, performance metrics, and audit trails.',
    category: 'operations'
  },
  {
    id: 'inference',
    title: 'Inference',
    content: 'Interactive inference testing. Test adapter performance and model outputs in real-time.',
    category: 'operations'
  },
  {
    id: 'alerts',
    title: 'Alerts',
    content: 'System alerts and monitoring. View active alerts, notifications, and system health warnings.',
    category: 'operations'
  },

  // Technical Terms
  {
    id: 'lora',
    title: 'LoRA',
    content: 'Low-Rank Adaptation. A technique for efficiently fine-tuning large language models with minimal parameters.',
    category: 'technical'
  },
  {
    id: 'adapter',
    title: 'Adapter',
    content: 'Specialized AI component that extends base model capabilities for specific domains or tasks.',
    category: 'technical'
  },
  {
    id: 'control-plane',
    title: 'Control Plane',
    content: 'The management layer that orchestrates adapter execution, policy enforcement, and system monitoring.',
    category: 'technical'
  },
  {
    id: 'tenant',
    title: 'Tenant',
    content: 'Isolated workspace with dedicated resources, policies, and data boundaries for multi-tenant operation.',
    category: 'technical'
  },
  {
    id: 'deterministic',
    title: 'Deterministic',
    content: 'System behavior that produces identical outputs for identical inputs, ensuring reproducible results.',
    category: 'technical'
  },
  {
    id: 'zero-egress',
    title: 'Zero Egress',
    content: 'Security mode that blocks all outbound network connections during serving to prevent data exfiltration.',
    category: 'technical'
  },
  {
    id: 'policy-pack',
    title: 'Policy Pack',
    content: 'Collection of security and compliance rules that enforce system behavior and data handling.',
    category: 'technical'
  },
  {
    id: 'telemetry-bundle',
    title: 'Telemetry Bundle',
    content: 'Compressed collection of system events, metrics, and audit logs for monitoring and compliance.',
    category: 'technical'
  },
  {
    id: 'router',
    title: 'Router',
    content: 'Component that selects the best adapters for each request based on context and performance.',
    category: 'technical'
  },
  {
    id: 'k-sparse',
    title: 'K-Sparse',
    content: 'Routing strategy that selects only the top K most relevant adapters to optimize performance.',
    category: 'technical'
  }
];

export function getHelpText(id: string): HelpTextItem | undefined {
  return helpTextDatabase.find(item => item.id === id);
}

export function getHelpTextByCategory(category: HelpTextItem['category']): HelpTextItem[] {
  return helpTextDatabase.filter(item => item.category === category);
}
