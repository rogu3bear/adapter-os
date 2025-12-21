import React, { useState, useEffect, useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { ConfirmationDialog, ConfirmationOptions } from '@/components/ui/confirmation-dialog';
import {
  FileText,
  Plus,
  Edit,
  Trash2,
  Copy,
  Download,
  Upload,
  Search,
  Code,
  BookOpen,
  Bug,
  FileCode,
  RefreshCw,
  Sparkles,
  X,
  Check,
  AlertTriangle,
} from 'lucide-react';
import { toast } from 'sonner';
import { Alert, AlertDescription } from '@/components/ui/alert';

export interface PromptTemplate {
  id: string;
  name: string;
  description: string;
  content: string;
  category: TemplateCategoryType;
  variables: string[];
  createdAt: string;
  updatedAt: string;
  isBuiltIn?: boolean;
}

export type TemplateCategoryType =
  | 'code-review'
  | 'documentation'
  | 'testing'
  | 'debugging'
  | 'refactoring'
  | 'custom';

interface CategoryConfig {
  id: TemplateCategoryType;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  color: string;
}

const CATEGORIES: CategoryConfig[] = [
  { id: 'code-review', label: 'Code Review', icon: Code, color: 'bg-blue-500' },
  { id: 'documentation', label: 'Documentation', icon: BookOpen, color: 'bg-green-500' },
  { id: 'testing', label: 'Testing', icon: FileCode, color: 'bg-purple-500' },
  { id: 'debugging', label: 'Debugging', icon: Bug, color: 'bg-red-500' },
  { id: 'refactoring', label: 'Refactoring', icon: RefreshCw, color: 'bg-yellow-500' },
  { id: 'custom', label: 'Custom', icon: Sparkles, color: 'bg-gray-500' },
];

const BUILT_IN_TEMPLATES: Omit<PromptTemplate, 'id' | 'createdAt' | 'updatedAt'>[] = [
  {
    name: 'Code Review',
    description: 'Comprehensive code review focusing on best practices and potential issues',
    category: 'code-review',
    content: `Please review the following {{language}} code and provide feedback on:
1. Code quality and best practices
2. Potential bugs or edge cases
3. Performance considerations
4. Security concerns
5. Suggestions for improvement

Code:
{{code}}

Focus areas (optional):
{{focus_areas}}`,
    variables: ['language', 'code', 'focus_areas'],
    isBuiltIn: true,
  },
  {
    name: 'Documentation Generator',
    description: 'Generate comprehensive documentation for code',
    category: 'documentation',
    content: `Generate detailed documentation for the following {{language}} code:

Code:
{{code}}

Include:
- Function/class purpose
- Parameters and return values
- Usage examples
- Edge cases and limitations
- Related functions or dependencies

Documentation style: {{style}}`,
    variables: ['language', 'code', 'style'],
    isBuiltIn: true,
  },
  {
    name: 'Unit Test Generator',
    description: 'Generate comprehensive unit tests for code',
    category: 'testing',
    content: `Generate comprehensive unit tests for the following {{language}} code using {{test_framework}}:

Code:
{{code}}

Requirements:
- Test happy path scenarios
- Test edge cases and error conditions
- Test boundary values
- Include setup and teardown if needed
- Add descriptive test names and comments

Coverage target: {{coverage_target}}%`,
    variables: ['language', 'code', 'test_framework', 'coverage_target'],
    isBuiltIn: true,
  },
  {
    name: 'Bug Analysis',
    description: 'Analyze error messages and suggest fixes',
    category: 'debugging',
    content: `Analyze the following error/bug in {{language}}:

Error message:
{{error_message}}

Code context:
{{code_context}}

Environment:
{{environment}}

Please provide:
1. Root cause analysis
2. Step-by-step debugging approach
3. Potential fixes with code examples
4. Prevention strategies for similar issues`,
    variables: ['language', 'error_message', 'code_context', 'environment'],
    isBuiltIn: true,
  },
  {
    name: 'Refactoring Assistant',
    description: 'Suggest refactoring improvements for code',
    category: 'refactoring',
    content: `Analyze the following {{language}} code and suggest refactoring improvements:

Code:
{{code}}

Focus on:
- Code readability and maintainability
- Design patterns and architecture
- Performance optimization
- Reducing complexity
- Improving testability

Constraints:
{{constraints}}

Please provide refactored code with explanations.`,
    variables: ['language', 'code', 'constraints'],
    isBuiltIn: true,
  },
  {
    name: 'API Design Review',
    description: 'Review API design and suggest improvements',
    category: 'code-review',
    content: `Review the following API design and provide feedback:

API Type: {{api_type}}
Endpoint/Interface:
{{api_spec}}

Consider:
1. RESTful principles (if applicable)
2. Naming conventions
3. Request/response structure
4. Error handling
5. Versioning strategy
6. Authentication/authorization
7. Rate limiting
8. Documentation completeness

Target use case: {{use_case}}`,
    variables: ['api_type', 'api_spec', 'use_case'],
    isBuiltIn: true,
  },
  {
    name: 'Security Audit',
    description: 'Perform security review of code',
    category: 'code-review',
    content: `Perform a security audit of the following {{language}} code:

Code:
{{code}}

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
    isBuiltIn: true,
  },
  {
    name: 'Performance Optimization',
    description: 'Identify and fix performance bottlenecks',
    category: 'refactoring',
    content: `Analyze the following {{language}} code for performance optimization:

Code:
{{code}}

Performance metrics:
{{metrics}}

Focus areas:
1. Algorithm complexity
2. Memory usage
3. Database queries
4. Caching opportunities
5. Async/parallel processing
6. Resource cleanup

Target performance goals: {{goals}}`,
    variables: ['language', 'code', 'metrics', 'goals'],
    isBuiltIn: true,
  },
  {
    name: 'Integration Test Generator',
    description: 'Generate integration tests for APIs or services',
    category: 'testing',
    content: `Generate integration tests for the following {{service_type}}:

Service specification:
{{spec}}

Test framework: {{test_framework}}

Include:
1. Setup and teardown procedures
2. Success scenarios
3. Error handling tests
4. Edge cases
5. Mock/stub dependencies
6. Assertions for expected behavior

Environment: {{environment}}`,
    variables: ['service_type', 'spec', 'test_framework', 'environment'],
    isBuiltIn: true,
  },
  {
    name: 'Code Explanation',
    description: 'Explain complex code in simple terms',
    category: 'documentation',
    content: `Explain the following {{language}} code in simple terms:

Code:
{{code}}

Audience level: {{audience_level}}

Provide:
1. High-level overview
2. Step-by-step explanation
3. Key concepts and terminology
4. Visual representation (if applicable)
5. Common use cases
6. Related patterns or techniques`,
    variables: ['language', 'code', 'audience_level'],
    isBuiltIn: true,
  },
];

const STORAGE_KEY = 'aos_prompt_templates';

function extractVariables(content: string): string[] {
  const regex = /\{\{(\w+)\}\}/g;
  const variables = new Set<string>();
  let match;

  while ((match = regex.exec(content)) !== null) {
    variables.add(match[1]);
  }

  return Array.from(variables).sort();
}

function initializeTemplates(): PromptTemplate[] {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored) {
    try {
      return JSON.parse(stored);
    } catch (error) {
      console.error('Failed to parse stored templates:', error);
    }
  }

  // Initialize with built-in templates
  const now = new Date().toISOString();
  return BUILT_IN_TEMPLATES.map((template, index) => ({
    ...template,
    id: `built-in-${index}`,
    createdAt: now,
    updatedAt: now,
  }));
}

interface PromptTemplateManagerProps {
  onApplyTemplate?: (template: PromptTemplate, substitutedContent: string) => void;
  isOpen: boolean;
  onClose: () => void;
}

export function PromptTemplateManager({
  onApplyTemplate,
  isOpen,
  onClose,
}: PromptTemplateManagerProps) {
  const [templates, setTemplates] = useState<PromptTemplate[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedCategory, setSelectedCategory] = useState<TemplateCategoryType | 'all'>('all');
  const [isEditorOpen, setIsEditorOpen] = useState(false);
  const [editingTemplate, setEditingTemplate] = useState<PromptTemplate | null>(null);
  const [isPreviewOpen, setIsPreviewOpen] = useState(false);
  const [previewTemplate, setPreviewTemplate] = useState<PromptTemplate | null>(null);
  const [variableValues, setVariableValues] = useState<Record<string, string>>({});
  const [confirmDelete, setConfirmDelete] = useState<ConfirmationOptions | null>(null);
  const [templateToDelete, setTemplateToDelete] = useState<string | null>(null);

  // Load templates on mount
  useEffect(() => {
    setTemplates(initializeTemplates());
  }, []);

  // Save templates to localStorage whenever they change
  useEffect(() => {
    if (templates.length > 0) {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(templates));
    }
  }, [templates]);

  const filteredTemplates = useMemo(() => {
    return templates.filter((template) => {
      const matchesSearch =
        searchQuery === '' ||
        template.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
        template.description.toLowerCase().includes(searchQuery.toLowerCase());

      const matchesCategory =
        selectedCategory === 'all' || template.category === selectedCategory;

      return matchesSearch && matchesCategory;
    });
  }, [templates, searchQuery, selectedCategory]);

  const handleCreateTemplate = () => {
    setEditingTemplate({
      id: '',
      name: '',
      description: '',
      content: '',
      category: 'custom',
      variables: [],
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    });
    setIsEditorOpen(true);
  };

  const handleEditTemplate = (template: PromptTemplate) => {
    if (template.isBuiltIn) {
      // Clone built-in template for editing
      setEditingTemplate({
        ...template,
        id: '',
        name: `${template.name} (Copy)`,
        isBuiltIn: false,
      });
    } else {
      setEditingTemplate(template);
    }
    setIsEditorOpen(true);
  };

  const handleSaveTemplate = (template: PromptTemplate) => {
    const now = new Date().toISOString();

    if (template.id) {
      // Update existing template
      setTemplates((prev) =>
        prev.map((t) =>
          t.id === template.id
            ? { ...template, updatedAt: now }
            : t
        )
      );
      toast.success('Template updated successfully');
    } else {
      // Create new template
      const newTemplate = {
        ...template,
        id: `custom-${Date.now()}`,
        createdAt: now,
        updatedAt: now,
      };
      setTemplates((prev) => [...prev, newTemplate]);
      toast.success('Template created successfully');
    }

    setIsEditorOpen(false);
    setEditingTemplate(null);
  };

  const handleDeleteTemplate = (templateId: string) => {
    const template = templates.find((t) => t.id === templateId);
    if (!template) return;

    if (template.isBuiltIn) {
      toast.error('Cannot delete built-in templates');
      return;
    }

    setTemplateToDelete(templateId);
    setConfirmDelete({
      title: 'Delete Template',
      description: `Are you sure you want to delete "${template.name}"? This action cannot be undone.`,
      confirmText: 'Delete',
      cancelText: 'Cancel',
      variant: 'destructive',
    });
  };

  const confirmDeleteTemplate = () => {
    if (templateToDelete) {
      setTemplates((prev) => prev.filter((t) => t.id !== templateToDelete));
      toast.success('Template deleted successfully');
      setTemplateToDelete(null);
      setConfirmDelete(null);
    }
  };

  const handleDuplicateTemplate = (template: PromptTemplate) => {
    const now = new Date().toISOString();
    const newTemplate = {
      ...template,
      id: `custom-${Date.now()}`,
      name: `${template.name} (Copy)`,
      isBuiltIn: false,
      createdAt: now,
      updatedAt: now,
    };
    setTemplates((prev) => [...prev, newTemplate]);
    toast.success('Template duplicated successfully');
  };

  const handlePreviewTemplate = (template: PromptTemplate) => {
    setPreviewTemplate(template);
    // Initialize variable values
    const initialValues: Record<string, string> = {};
    template.variables.forEach((variable) => {
      initialValues[variable] = '';
    });
    setVariableValues(initialValues);
    setIsPreviewOpen(true);
  };

  const substituteVariables = (content: string, values: Record<string, string>): string => {
    let result = content;
    Object.entries(values).forEach(([key, value]) => {
      const regex = new RegExp(`\\{\\{${key}\\}\\}`, 'g');
      result = result.replace(regex, value || `{{${key}}}`);
    });
    return result;
  };

  const handleApplyFromPreview = () => {
    if (previewTemplate && onApplyTemplate) {
      const substituted = substituteVariables(previewTemplate.content, variableValues);
      onApplyTemplate(previewTemplate, substituted);
      setIsPreviewOpen(false);
      onClose();
      toast.success('Template applied to prompt');
    }
  };

  const handleExportTemplates = () => {
    const customTemplates = templates.filter((t) => !t.isBuiltIn);
    const data = JSON.stringify(customTemplates, null, 2);
    const blob = new Blob([data], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `prompt-templates-${Date.now()}.json`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
    toast.success('Templates exported successfully');
  };

  const handleImportTemplates = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = (e) => {
      try {
        const imported = JSON.parse(e.target?.result as string) as PromptTemplate[];
        const now = new Date().toISOString();

        // Assign new IDs and timestamps to imported templates
        const newTemplates = imported.map((template) => ({
          ...template,
          id: `imported-${Date.now()}-${Math.random()}`,
          createdAt: now,
          updatedAt: now,
          isBuiltIn: false,
        }));

        setTemplates((prev) => [...prev, ...newTemplates]);
        toast.success(`Imported ${newTemplates.length} template(s)`);
      } catch (error) {
        toast.error('Failed to import templates. Invalid file format.');
      }
    };
    reader.readAsText(file);
    // Reset input to allow importing the same file again
    event.target.value = '';
  };

  const getCategoryConfig = (category: TemplateCategoryType): CategoryConfig => {
    return CATEGORIES.find((c) => c.id === category) || CATEGORIES[CATEGORIES.length - 1];
  };

  return (
    <>
      <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
        <DialogContent className="max-w-5xl max-h-[90vh] overflow-hidden flex flex-col">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <FileText className="h-5 w-5" />
              Prompt Template Manager
            </DialogTitle>
            <DialogDescription>
              Create, manage, and apply reusable prompt templates with variable substitution
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-4 flex-1 overflow-hidden">
            {/* Toolbar */}
            <div className="flex flex-col sm:flex-row gap-3">
              <div className="flex-1 relative">
                <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                <Input
                  placeholder="Search templates..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="pl-10"
                />
              </div>

              <Select
                value={selectedCategory}
                onValueChange={(value) => setSelectedCategory(value as TemplateCategoryType | 'all')}
              >
                <SelectTrigger className="w-[180px]">
                  <SelectValue placeholder="All Categories" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All Categories</SelectItem>
                  {CATEGORIES.map((category) => (
                    <SelectItem key={category.id} value={category.id}>
                      {category.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>

              <Button onClick={handleCreateTemplate} size="sm">
                <Plus className="h-4 w-4 mr-2" />
                New Template
              </Button>

              <div className="flex gap-2">
                <Button variant="outline" size="sm" onClick={handleExportTemplates}>
                  <Download className="h-4 w-4" />
                </Button>
                <Button variant="outline" size="sm" asChild>
                  <label className="cursor-pointer">
                    <Upload className="h-4 w-4" />
                    <input
                      type="file"
                      accept=".json"
                      className="hidden"
                      onChange={handleImportTemplates}
                    />
                  </label>
                </Button>
              </div>
            </div>

            {/* Category Filter Pills */}
            <div className="flex gap-2 flex-wrap">
              <Badge
                variant={selectedCategory === 'all' ? 'default' : 'outline'}
                className="cursor-pointer"
                onClick={() => setSelectedCategory('all')}
              >
                All ({templates.length})
              </Badge>
              {CATEGORIES.map((category) => {
                const count = templates.filter((t) => t.category === category.id).length;
                return (
                  <Badge
                    key={category.id}
                    variant={selectedCategory === category.id ? 'default' : 'outline'}
                    className="cursor-pointer"
                    onClick={() => setSelectedCategory(category.id)}
                  >
                    {category.label} ({count})
                  </Badge>
                );
              })}
            </div>

            {/* Template List */}
            <div className="flex-1 overflow-y-auto space-y-3">
              {filteredTemplates.length === 0 ? (
                <div className="flex flex-col items-center justify-center py-12 text-center">
                  <FileText className="h-12 w-12 text-muted-foreground mb-4" />
                  <p className="text-muted-foreground">
                    {searchQuery || selectedCategory !== 'all'
                      ? 'No templates match your filters'
                      : 'No templates yet. Create your first template!'}
                  </p>
                </div>
              ) : (
                filteredTemplates.map((template) => {
                  const categoryConfig = getCategoryConfig(template.category);
                  const CategoryIcon = categoryConfig.icon;

                  return (
                    <Card key={template.id} className="hover:shadow-md transition-shadow">
                      <CardHeader className="pb-3">
                        <div className="flex items-start justify-between">
                          <div className="flex-1">
                            <div className="flex items-center gap-2 mb-1">
                              <CategoryIcon className="h-4 w-4 text-muted-foreground" />
                              <CardTitle className="text-base">{template.name}</CardTitle>
                              {template.isBuiltIn && (
                                <Badge variant="secondary" className="text-xs">
                                  Built-in
                                </Badge>
                              )}
                            </div>
                            <CardDescription className="text-sm">
                              {template.description}
                            </CardDescription>
                          </div>

                          <div className="flex gap-1 ml-4">
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => handlePreviewTemplate(template)}
                              title="Preview and apply"
                            >
                              <FileText className="h-4 w-4" />
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => handleEditTemplate(template)}
                              title={template.isBuiltIn ? 'Duplicate and edit' : 'Edit'}
                            >
                              <Edit className="h-4 w-4" />
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => handleDuplicateTemplate(template)}
                              title="Duplicate"
                            >
                              <Copy className="h-4 w-4" />
                            </Button>
                            {!template.isBuiltIn && (
                              <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => handleDeleteTemplate(template.id)}
                                title="Delete"
                              >
                                <Trash2 className="h-4 w-4 text-destructive" />
                              </Button>
                            )}
                          </div>
                        </div>
                      </CardHeader>

                      <CardContent className="pt-0">
                        <div className="flex flex-wrap gap-1">
                          <Badge variant="outline" className="text-xs">
                            <span className={`w-2 h-2 rounded-full ${categoryConfig.color} mr-1`} />
                            {categoryConfig.label}
                          </Badge>
                          {template.variables.length > 0 && (
                            <>
                              {template.variables.slice(0, 3).map((variable) => (
                                <Badge key={variable} variant="secondary" className="text-xs">
                                  {variable}
                                </Badge>
                              ))}
                              {template.variables.length > 3 && (
                                <Badge variant="secondary" className="text-xs">
                                  +{template.variables.length - 3} more
                                </Badge>
                              )}
                            </>
                          )}
                        </div>
                      </CardContent>
                    </Card>
                  );
                })
              )}
            </div>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={onClose}>
              Close
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Template Editor Dialog */}
      <TemplateEditor
        template={editingTemplate}
        isOpen={isEditorOpen}
        onClose={() => {
          setIsEditorOpen(false);
          setEditingTemplate(null);
        }}
        onSave={handleSaveTemplate}
      />

      {/* Template Preview Dialog */}
      <TemplatePreview
        template={previewTemplate}
        isOpen={isPreviewOpen}
        onClose={() => {
          setIsPreviewOpen(false);
          setPreviewTemplate(null);
          setVariableValues({});
        }}
        variableValues={variableValues}
        onVariableChange={setVariableValues}
        onApply={handleApplyFromPreview}
      />

      {/* Delete Confirmation */}
      {confirmDelete && (
        <ConfirmationDialog
          open={!!confirmDelete}
          onOpenChange={(open) => {
            if (!open) {
              setConfirmDelete(null);
              setTemplateToDelete(null);
            }
          }}
          onConfirm={confirmDeleteTemplate}
          options={confirmDelete}
        />
      )}
    </>
  );
}

interface TemplateEditorProps {
  template: PromptTemplate | null;
  isOpen: boolean;
  onClose: () => void;
  onSave: (template: PromptTemplate) => void;
}

function TemplateEditor({ template, isOpen, onClose, onSave }: TemplateEditorProps) {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [content, setContent] = useState('');
  const [category, setCategory] = useState<TemplateCategoryType>('custom');
  const [detectedVariables, setDetectedVariables] = useState<string[]>([]);
  const [validationError, setValidationError] = useState<string | null>(null);

  useEffect(() => {
    if (template) {
      setName(template.name);
      setDescription(template.description);
      setContent(template.content);
      setCategory(template.category);
      setDetectedVariables(template.variables);
    } else {
      setName('');
      setDescription('');
      setContent('');
      setCategory('custom');
      setDetectedVariables([]);
    }
    setValidationError(null);
  }, [template]);

  useEffect(() => {
    const variables = extractVariables(content);
    setDetectedVariables(variables);
  }, [content]);

  const handleSave = () => {
    // Validation
    if (!name.trim()) {
      setValidationError('Template name is required');
      return;
    }
    if (!description.trim()) {
      setValidationError('Template description is required');
      return;
    }
    if (!content.trim()) {
      setValidationError('Template content is required');
      return;
    }

    const savedTemplate: PromptTemplate = {
      id: template?.id || '',
      name: name.trim(),
      description: description.trim(),
      content: content.trim(),
      category,
      variables: detectedVariables,
      createdAt: template?.createdAt || new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      isBuiltIn: false,
    };

    onSave(savedTemplate);
  };

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-3xl max-h-[90vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle>
            {template?.id ? 'Edit Template' : 'Create Template'}
          </DialogTitle>
          <DialogDescription>
            Use {'{{variable}}'} syntax for placeholders. Variables will be detected automatically.
          </DialogDescription>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-4">
          {validationError && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>{validationError}</AlertDescription>
            </Alert>
          )}

          <div className="space-y-2">
            <Label htmlFor="template-name">Template Name</Label>
            <Input
              id="template-name"
              placeholder="e.g., Code Review"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="template-description">Description</Label>
            <Input
              id="template-description"
              placeholder="Brief description of what this template does"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="template-category">Category</Label>
            <Select value={category} onValueChange={(value) => setCategory(value as TemplateCategoryType)}>
              <SelectTrigger id="template-category">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {CATEGORIES.map((cat) => {
                  const Icon = cat.icon;
                  return (
                    <SelectItem key={cat.id} value={cat.id}>
                      <div className="flex items-center gap-2">
                        <Icon className="h-4 w-4" />
                        {cat.label}
                      </div>
                    </SelectItem>
                  );
                })}
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <Label htmlFor="template-content">Template Content</Label>
            <Textarea
              id="template-content"
              placeholder="Enter your template content with {{variable}} placeholders..."
              value={content}
              onChange={(e) => setContent(e.target.value)}
              rows={12}
              className="font-mono text-sm"
            />
            <p className="text-xs text-muted-foreground">
              Use {'{{variable_name}}'} for placeholders. Example: "Review this {'{{language}}'} code: {'{{code}}'}"
            </p>
          </div>

          {detectedVariables.length > 0 && (
            <div className="space-y-2">
              <Label>Detected Variables ({detectedVariables.length})</Label>
              <div className="flex flex-wrap gap-2">
                {detectedVariables.map((variable) => (
                  <Badge key={variable} variant="secondary">
                    {variable}
                  </Badge>
                ))}
              </div>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={handleSave}>
            <Check className="h-4 w-4 mr-2" />
            {template?.id ? 'Save Changes' : 'Create Template'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

interface TemplatePreviewProps {
  template: PromptTemplate | null;
  isOpen: boolean;
  onClose: () => void;
  variableValues: Record<string, string>;
  onVariableChange: (values: Record<string, string>) => void;
  onApply: () => void;
}

function TemplatePreview({
  template,
  isOpen,
  onClose,
  variableValues,
  onVariableChange,
  onApply,
}: TemplatePreviewProps) {
  if (!template) return null;

  const substitutedContent = useMemo(() => {
    let result = template.content;
    Object.entries(variableValues).forEach(([key, value]) => {
      const regex = new RegExp(`\\{\\{${key}\\}\\}`, 'g');
      result = result.replace(regex, value || `{{${key}}}`);
    });
    return result;
  }, [template.content, variableValues]);

  const hasEmptyVariables = template.variables.some((v) => !variableValues[v]?.trim());

  const handleCopyToClipboard = () => {
    navigator.clipboard.writeText(substitutedContent);
    toast.success('Copied to clipboard');
  };

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <FileText className="h-5 w-5" />
            {template.name}
          </DialogTitle>
          <DialogDescription>{template.description}</DialogDescription>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-4">
          {template.variables.length > 0 && (
            <div className="space-y-3">
              <Label className="text-base">Fill in Variables</Label>
              {template.variables.map((variable) => (
                <div key={variable} className="space-y-1">
                  <Label htmlFor={`var-${variable}`} className="text-sm font-normal">
                    {variable.replace(/_/g, ' ')}
                  </Label>
                  <Textarea
                    id={`var-${variable}`}
                    placeholder={`Enter value for ${variable}...`}
                    value={variableValues[variable] || ''}
                    onChange={(e) =>
                      onVariableChange({
                        ...variableValues,
                        [variable]: e.target.value,
                      })
                    }
                    rows={3}
                    className="text-sm"
                  />
                </div>
              ))}
            </div>
          )}

          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <Label className="text-base">Preview</Label>
              <Button variant="ghost" size="sm" onClick={handleCopyToClipboard}>
                <Copy className="h-4 w-4 mr-2" />
                Copy
              </Button>
            </div>
            <div className="relative">
              <pre className="whitespace-pre-wrap text-sm p-4 bg-muted border border-border rounded-lg font-mono">
                {substitutedContent}
              </pre>
              {hasEmptyVariables && (
                <div className="mt-2">
                  <Alert>
                    <AlertTriangle className="h-4 w-4" />
                    <AlertDescription>
                      Some variables are empty. Fill them in to see the complete preview.
                    </AlertDescription>
                  </Alert>
                </div>
              )}
            </div>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={onApply} disabled={!template.variables.every((v) => variableValues[v]?.trim())}>
            <Check className="h-4 w-4 mr-2" />
            Apply to Prompt
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export default PromptTemplateManager;
