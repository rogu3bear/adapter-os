/**
 * Hook for managing prompt templates
 * Handles CRUD operations, persistence, and variable detection
 */

import { useState, useCallback, useEffect } from 'react';

export interface PromptTemplate {
  id: string;
  name: string;
  description: string;
  prompt: string;
  category: string;
  variables: string[];
  created_at: string;
  updated_at: string;
  isFavorite?: boolean;
}

export interface TemplateVariable {
  name: string;
  placeholder: string;
  defaultValue?: string;
}

const STORAGE_KEY = 'aos_prompt_templates';
const RECENT_TEMPLATES_KEY = 'aos_recent_templates';

// Detect variables in template (matches {{variable}} or {variable})
const detectVariables = (prompt: string): string[] => {
  const patterns = [
    /\{\{(\w+)\}\}/g,  // {{variable}}
    /\{(\w+)\}/g,       // {variable}
  ];

  const variables = new Set<string>();

  patterns.forEach(pattern => {
    let match;
    while ((match = pattern.exec(prompt)) !== null) {
      variables.add(match[1]);
    }
  });

  return Array.from(variables);
};

// Default templates
const DEFAULT_TEMPLATES: PromptTemplate[] = [
  {
    id: 'code-review',
    name: 'Code Review',
    description: 'Comprehensive code review focusing on best practices',
    category: 'engineering',
    prompt: `Please review the following {{language}} code and provide feedback on:
1. Code quality and best practices
2. Potential bugs or edge cases
3. Performance considerations
4. Security concerns
5. Suggestions for improvement

Code:
\`\`\`{{language}}
{{code}}
\`\`\`

Focus areas (optional): {{focus_areas}}`,
    variables: ['language', 'code', 'focus_areas'],
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
  {
    id: 'documentation',
    name: 'Documentation Generator',
    description: 'Generate comprehensive documentation for code',
    category: 'engineering',
    prompt: `Generate detailed documentation for the following {{language}} code:

Code:
\`\`\`{{language}}
{{code}}
\`\`\`

Include:
- Function/class purpose
- Parameters and return values
- Usage examples
- Edge cases and limitations
- Related functions or dependencies

Documentation style: {{style}}`,
    variables: ['language', 'code', 'style'],
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
  {
    id: 'unit-tests',
    name: 'Unit Test Generator',
    description: 'Generate comprehensive unit tests',
    category: 'engineering',
    prompt: `Generate comprehensive unit tests for the following {{language}} code using {{test_framework}}:

Code:
\`\`\`{{language}}
{{code}}
\`\`\`

Requirements:
- Test happy path scenarios
- Test edge cases and error conditions
- Test boundary values
- Include setup and teardown if needed
- Add descriptive test names and comments

Coverage target: {{coverage}}%`,
    variables: ['language', 'code', 'test_framework', 'coverage'],
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
  {
    id: 'bug-analysis',
    name: 'Bug Analysis',
    description: 'Analyze error messages and suggest fixes',
    category: 'engineering',
    prompt: `Analyze the following error/bug in {{language}}:

Error message:
{{error_message}}

Code context:
\`\`\`{{language}}
{{code_context}}
\`\`\`

Environment: {{environment}}

Please provide:
1. Root cause analysis
2. Step-by-step debugging approach
3. Potential fixes with code examples
4. Prevention strategies for similar issues`,
    variables: ['language', 'error_message', 'code_context', 'environment'],
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
  {
    id: 'refactoring',
    name: 'Refactoring Assistant',
    description: 'Suggest refactoring improvements',
    category: 'engineering',
    prompt: `Analyze the following {{language}} code and suggest refactoring improvements:

Code:
\`\`\`{{language}}
{{code}}
\`\`\`

Focus on:
- Code readability and maintainability
- Design patterns and architecture
- Performance optimization
- Reducing complexity
- Improving testability

Constraints: {{constraints}}

Please provide refactored code with explanations.`,
    variables: ['language', 'code', 'constraints'],
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
  {
    id: 'security-audit',
    name: 'Security Audit',
    description: 'Perform security review of code',
    category: 'engineering',
    prompt: `Perform a security audit of the following {{language}} code:

Code:
\`\`\`{{language}}
{{code}}
\`\`\`

Check for:
1. Input validation vulnerabilities
2. SQL injection risks
3. XSS vulnerabilities
4. Authentication/authorization issues
5. Sensitive data exposure
6. Insecure dependencies
7. Cryptographic weaknesses
8. Error information leakage

Deployment environment: {{environment}}`,
    variables: ['language', 'code', 'environment'],
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
  {
    id: 'performance-optimization',
    name: 'Performance Optimization',
    description: 'Identify and fix performance bottlenecks',
    category: 'engineering',
    prompt: `Analyze the following {{language}} code for performance optimization:

Code:
\`\`\`{{language}}
{{code}}
\`\`\`

Performance metrics: {{metrics}}

Focus areas:
1. Algorithm complexity
2. Memory usage
3. Database queries
4. Caching opportunities
5. Async/parallel processing
6. Resource cleanup

Target performance goals: {{goals}}`,
    variables: ['language', 'code', 'metrics', 'goals'],
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
  {
    id: 'code-explanation',
    name: 'Code Explanation',
    description: 'Explain complex code in simple terms',
    category: 'education',
    prompt: `Explain the following {{language}} code in simple terms:

Code:
\`\`\`{{language}}
{{code}}
\`\`\`

Audience level: {{audience_level}}

Provide:
1. High-level overview
2. Step-by-step explanation
3. Key concepts and terminology
4. Visual representation (if applicable)
5. Common use cases
6. Related patterns or techniques`,
    variables: ['language', 'code', 'audience_level'],
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
  {
    id: 'summarize',
    name: 'Summarize Text',
    description: 'Create a concise summary',
    category: 'writing',
    prompt: 'Summarize the following text in {{length}} sentences, focusing on {{focus}}:\n\n{{text}}',
    variables: ['length', 'focus', 'text'],
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
  {
    id: 'explain',
    name: 'Explain Concept',
    description: 'Explain a concept clearly',
    category: 'education',
    prompt: 'Explain {{concept}} in a way that a {{audience}} would understand. Use {{style}} and provide examples.',
    variables: ['concept', 'audience', 'style'],
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
];

export function usePromptTemplates() {
  const [templates, setTemplates] = useState<PromptTemplate[]>([]);
  const [recentTemplates, setRecentTemplates] = useState<string[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Load templates from localStorage on mount
  useEffect(() => {
    const loadTemplates = () => {
      try {
        setIsLoading(true);
        const stored = localStorage.getItem(STORAGE_KEY);
        const recentStored = localStorage.getItem(RECENT_TEMPLATES_KEY);

        if (stored) {
          const parsed = JSON.parse(stored) as PromptTemplate[];
          setTemplates(parsed);
        } else {
          // Initialize with defaults
          setTemplates(DEFAULT_TEMPLATES);
          saveTemplatesToStorage(DEFAULT_TEMPLATES);
        }

        if (recentStored) {
          setRecentTemplates(JSON.parse(recentStored));
        }
        setError(null);
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to load templates';
        setError(message);
        // Fallback to defaults on error
        setTemplates(DEFAULT_TEMPLATES);
      } finally {
        setIsLoading(false);
      }
    };

    loadTemplates();
  }, []);

  const saveTemplatesToStorage = (templatesData: PromptTemplate[]) => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(templatesData));
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to save templates';
      setError(message);
    }
  };

  const saveRecentTemplates = (templateIds: string[]) => {
    try {
      localStorage.setItem(RECENT_TEMPLATES_KEY, JSON.stringify(templateIds.slice(0, 5)));
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to save recent templates';
      setError(message);
    }
  };

  // Create new template
  const createTemplate = useCallback((
    name: string,
    description: string,
    prompt: string,
    category: string
  ): PromptTemplate => {
    const newTemplate: PromptTemplate = {
      id: `template-${Date.now()}`,
      name,
      description,
      prompt,
      category,
      variables: detectVariables(prompt),
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    };

    const updated = [newTemplate, ...templates];
    setTemplates(updated);
    saveTemplatesToStorage(updated);

    return newTemplate;
  }, [templates]);

  // Update template
  const updateTemplate = useCallback((
    id: string,
    updates: Partial<Omit<PromptTemplate, 'id' | 'created_at'>>
  ): PromptTemplate | null => {
    const template = templates.find(t => t.id === id);
    if (!template) return null;

    const updated = {
      ...template,
      ...updates,
      updated_at: new Date().toISOString(),
      variables: updates.prompt
        ? detectVariables(updates.prompt)
        : template.variables,
    };

    const newTemplates = templates.map(t => t.id === id ? updated : t);
    setTemplates(newTemplates);
    saveTemplatesToStorage(newTemplates);

    return updated;
  }, [templates]);

  // Delete template
  const deleteTemplate = useCallback((id: string): boolean => {
    const newTemplates = templates.filter(t => t.id !== id);
    setTemplates(newTemplates);
    saveTemplatesToStorage(newTemplates);

    // Remove from recent
    setRecentTemplates(prev => prev.filter(tid => tid !== id));
    saveRecentTemplates(recentTemplates.filter(tid => tid !== id));

    return true;
  }, [templates, recentTemplates]);

  // Get template by ID
  const getTemplate = useCallback((id: string): PromptTemplate | undefined => {
    return templates.find(t => t.id === id);
  }, [templates]);

  // Get all templates or filtered by category
  const getTemplates = useCallback((category?: string): PromptTemplate[] => {
    if (!category) return templates;
    return templates.filter(t => t.category === category);
  }, [templates]);

  // Get recent templates
  const getRecentTemplates = useCallback((): PromptTemplate[] => {
    return recentTemplates
      .map(id => templates.find(t => t.id === id))
      .filter((t): t is PromptTemplate => !!t);
  }, [recentTemplates, templates]);

  // Record template usage
  const recordTemplateUsage = useCallback((id: string) => {
    const newRecent = [id, ...recentTemplates.filter(rid => rid !== id)].slice(0, 5);
    setRecentTemplates(newRecent);
    saveRecentTemplates(newRecent);
  }, [recentTemplates]);

  // Toggle favorite
  const toggleFavorite = useCallback((id: string): PromptTemplate | null => {
    const template = templates.find(t => t.id === id);
    if (!template) return null;

    return updateTemplate(id, { isFavorite: !template.isFavorite });
  }, [templates, updateTemplate]);

  // Get unique categories
  const getCategories = useCallback((): string[] => {
    const categories = new Set(templates.map(t => t.category));
    return Array.from(categories).sort();
  }, [templates]);

  // Substitute variables in template
  const substituteVariables = useCallback((
    templateId: string,
    variables: Record<string, string>
  ): string | null => {
    const template = getTemplate(templateId);
    if (!template) return null;

    let result = template.prompt;

    // Replace {{variable}} and {variable} patterns
    Object.entries(variables).forEach(([key, value]) => {
      result = result.replace(new RegExp(`\\{\\{${key}\\}\\}`, 'g'), value);
      result = result.replace(new RegExp(`\\{${key}\\}`, 'g'), value);
    });

    return result;
  }, [getTemplate]);

  // Search templates
  const searchTemplates = useCallback((query: string): PromptTemplate[] => {
    const lowercaseQuery = query.toLowerCase();
    return templates.filter(t =>
      t.name.toLowerCase().includes(lowercaseQuery) ||
      t.description.toLowerCase().includes(lowercaseQuery) ||
      t.prompt.toLowerCase().includes(lowercaseQuery)
    );
  }, [templates]);

  // Export templates to JSON
  const exportTemplates = useCallback((): string => {
    // Only export custom templates (not built-in ones)
    const customTemplates = templates.filter(t => !t.id.startsWith('code-review') && !t.id.startsWith('documentation') && !t.id.startsWith('unit-tests') && !t.id.startsWith('bug-analysis') && !t.id.startsWith('refactoring') && !t.id.startsWith('security-audit') && !t.id.startsWith('performance-optimization') && !t.id.startsWith('code-explanation') && !t.id.startsWith('summarize') && !t.id.startsWith('explain'));
    return JSON.stringify(customTemplates, null, 2);
  }, [templates]);

  // Import templates from JSON
  const importTemplates = useCallback((jsonData: string): boolean => {
    try {
      const imported = JSON.parse(jsonData) as PromptTemplate[];

      // Validate imported data
      if (!Array.isArray(imported)) {
        throw new Error('Invalid template format');
      }

      // Assign new IDs and timestamps to avoid conflicts
      const now = new Date().toISOString();
      const newTemplates = imported.map(template => ({
        ...template,
        id: `imported-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
        created_at: now,
        updated_at: now,
        variables: detectVariables(template.prompt),
      }));

      const updated = [...templates, ...newTemplates];
      setTemplates(updated);
      saveTemplatesToStorage(updated);

      return true;
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to import templates';
      setError(message);
      return false;
    }
  }, [templates]);

  return {
    // State
    templates,
    recentTemplates,
    isLoading,
    error,

    // CRUD operations
    createTemplate,
    updateTemplate,
    deleteTemplate,
    getTemplate,
    getTemplates,
    getRecentTemplates,
    getCategories,

    // Utilities
    recordTemplateUsage,
    toggleFavorite,
    substituteVariables,
    searchTemplates,
    exportTemplates,
    importTemplates,
  };
}
