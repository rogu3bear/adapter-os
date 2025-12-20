export interface DocumentationEntry {
  id: string;
  slug?: string;
  title: string;
  path: string;
  anchor?: string;
  category: 'getting-started' | 'architecture' | 'api' | 'guides' | 'operations' | 'development';
  description: string;
  featured?: boolean;
  tags?: string[];
}

export const documentationIndex: DocumentationEntry[] = [
  // Getting Started
  {
    id: 'getting-started-diagrams',
    title: 'Getting Started with Diagrams',
    path: 'GETTING_STARTED_WITH_DIAGRAMS.md',
    category: 'getting-started',
    description: 'Beginner-friendly visual guide with diagrams and real-world examples',
    featured: true
  },
  {
    id: 'quickstart',
    title: 'Quick Start Guide',
    path: 'QUICKSTART.md',
    category: 'getting-started',
    description: 'Get up and running in 10 minutes with backend setup and web UI deployment',
    featured: true
  },
  {
    id: 'documentation-map',
    title: 'Documentation Map',
    path: 'DOCUMENTATION_MAP.md',
    category: 'getting-started',
    description: 'Visual guide to navigating all AdapterOS documentation',
    tags: ['map', 'overview', 'index']
  },
  {
    id: 'documentation-overview',
    slug: 'arch-index:overview',
    title: 'Documentation Overview',
    path: 'README.md',
    anchor: 'table-of-contents',
    category: 'getting-started',
    description: 'Table of contents for AdapterOS references with verification status',
    featured: true,
    tags: ['index', 'catalog']
  },
  
  // Architecture
  {
    id: 'architecture',
    title: 'System Architecture',
    path: 'architecture.md',
    category: 'architecture',
    description: 'High-level system design and component overview',
    featured: true
  },
  {
    id: 'precision-diagrams',
    title: 'Precision Diagrams',
    path: 'architecture/precision-diagrams.md',
    category: 'architecture',
    description: 'Code-verified architecture diagrams with exact crate names and file paths'
  },
  {
    id: 'control-plane',
    title: 'Control Plane',
    path: 'ARCHITECTURE.md',
    anchor: 'architecture-components',
    category: 'architecture',
    description: 'Control plane architecture and APIs'
  },
  {
    id: 'architecture-index',
    slug: 'arch-index',
    title: 'Architecture Index',
    path: 'ARCHITECTURE.md',
    category: 'architecture',
    description: 'Complete architecture reference',
    tags: ['architecture', 'reference', 'overview']
  },
  
  // API Reference
  {
    id: 'api',
    title: 'API Reference',
    path: 'api.md',
    category: 'api',
    description: 'Complete API documentation'
  },
  
  // Guides
  {
    id: 'mlx-integration',
    title: 'MLX Integration',
    path: 'MLX_INTEGRATION.md',
    category: 'guides',
    description: 'MLX integration guide for Apple Silicon'
  },
  {
    id: 'mlx-installation',
    title: 'MLX Installation Guide',
    path: 'MLX_INSTALLATION_GUIDE.md',
    category: 'guides',
    description: 'Step-by-step setup for real MLX backend on Apple Silicon'
  },
  {
    id: 'mlx-migration',
    title: 'MLX Migration (Stub → Real)',
    path: 'MLX_MIGRATION_GUIDE.md',
    category: 'guides',
    description: 'Checklist for moving from stub MLX to real backend'
  },
  {
    id: 'mlx-troubleshooting',
    title: 'MLX Troubleshooting',
    path: 'MLX_TROUBLESHOOTING.md',
    category: 'guides',
    description: 'Common MLX build/runtime issues and fixes'
  },
  {
    id: 'mlx-vs-coreml',
    title: 'MLX vs CoreML',
    path: 'MLX_VS_COREML_GUIDE.md',
    category: 'guides',
    description: 'Backend selection guidance across MLX, CoreML, and Metal'
  },
  {
    id: 'runaway-prevention',
    title: 'Runaway Prevention',
    path: 'runaway-prevention.md',
    category: 'guides',
    description: 'Safety mechanisms and runaway prevention'
  },
  {
    id: 'cursor-integration',
    title: 'Cursor Integration Guide',
    path: 'CURSOR_INTEGRATION_GUIDE.md',
    category: 'guides',
    description: 'Guide for Cursor IDE integration'
  },
  {
    id: 'mission-visual',
    slug: 'arch-index:visual',
    title: 'Mission Guide: Visual Learner',
    path: 'ARCHITECTURE.md',
    anchor: 'system-overview',
    category: 'guides',
    description: 'Diagram-first onboarding to understand AdapterOS visually',
    featured: true,
    tags: ['mission', 'visual', 'onboarding']
  },
  {
    id: 'mission-code-first',
    slug: 'arch-index:code-first',
    title: 'Mission Guide: Code-First Developer',
    path: 'ARCHITECTURE.md',
    anchor: 'core-concepts',
    category: 'guides',
    description: 'Hands-on route through repositories, diagrams, and API contracts',
    tags: ['mission', 'developer', 'code']
  },
  {
    id: 'mission-operations',
    slug: 'arch-index:operations',
    title: 'Mission Guide: Operations & SRE',
    path: 'ARCHITECTURE.md',
    anchor: 'architecture-components',
    category: 'guides',
    description: 'Operational flow for telemetry, incident response, and promotion pipelines',
    tags: ['mission', 'sre', 'operations']
  },
  {
    id: 'mission-security',
    slug: 'arch-index:security',
    title: 'Mission Guide: Security & Compliance',
    path: 'ARCHITECTURE.md',
    anchor: 'user-flows',
    category: 'guides',
    description: 'Security isolation model and compliance verification playbook',
    tags: ['mission', 'security', 'compliance']
  },
  
  // Operations
  {
    id: 'deployment',
    title: 'Deployment',
    path: 'DEPLOYMENT.md',
    category: 'operations',
    description: 'Deployment guides and procedures'
  },
  {
    id: 'production-readiness',
    title: 'Production Readiness',
    path: 'PRODUCTION_READINESS.md',
    category: 'operations',
    description: 'Production deployment checklist and guidelines'
  },
  
  // Development
  {
    id: 'code-intelligence',
    title: 'Code Intelligence',
    path: 'code-intelligence/README.md',
    category: 'development',
    description: 'Code intelligence system documentation'
  }
];

export function getDocumentationByCategory(category: DocumentationEntry['category']): DocumentationEntry[] {
  return documentationIndex.filter(doc => doc.category === category);
}

export function getFeaturedDocumentation(): DocumentationEntry[] {
  return documentationIndex.filter(doc => doc.featured);
}

export function searchDocumentation(query: string): DocumentationEntry[] {
  const q = query.toLowerCase().trim();
  if (!q) return documentationIndex;
  
  return documentationIndex.filter(doc => 
    doc.title.toLowerCase().includes(q) ||
    doc.description.toLowerCase().includes(q) ||
    doc.category.toLowerCase().includes(q) ||
    doc.tags?.some(tag => tag.toLowerCase().includes(q))
  );
}
