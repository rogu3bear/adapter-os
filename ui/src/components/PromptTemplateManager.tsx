/**
 * Prompt Template Manager Dialog
 * Provides CRUD interface for managing prompt templates
 */

import React, { useState, useCallback } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Textarea } from './ui/textarea';
import { Label } from './ui/label';
import { Badge } from './ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from './ui/select';
import { Alert, AlertDescription } from './ui/alert';
import {
  Plus,
  Edit2,
  Trash2,
  Star,
  Copy,
  X,
  Search,
  AlertTriangle,
  Check,
  ChevronLeft,
  Download,
  Upload,
  FileText,
} from 'lucide-react';
import { PromptTemplate, usePromptTemplates } from '@/hooks/chat/usePromptTemplates';
import { toast } from 'sonner';

interface PromptTemplateManagerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelectTemplate?: (template: PromptTemplate) => void;
}

type ViewMode = 'list' | 'create' | 'edit';

export function PromptTemplateManager({
  open,
  onOpenChange,
  onSelectTemplate,
}: PromptTemplateManagerProps) {
  const {
    templates,
    createTemplate,
    updateTemplate,
    deleteTemplate,
    toggleFavorite,
    getCategories,
    searchTemplates,
    substituteVariables,
    exportTemplates,
    importTemplates,
  } = usePromptTemplates();

  const [viewMode, setViewMode] = useState<ViewMode>('list');
  const [selectedTemplate, setSelectedTemplate] = useState<PromptTemplate | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedCategory, setSelectedCategory] = useState<string>('all');
  const [sortBy, setSortBy] = useState<'name' | 'recent' | 'favorite'>('recent');
  const [variableValues, setVariableValues] = useState<Record<string, string>>({});
  const [showVariableSubstitution, setShowVariableSubstitution] = useState(false);

  // Form state for create/edit
  const [formData, setFormData] = useState({
    name: '',
    description: '',
    prompt: '',
    category: 'general',
  });

  const categories = getCategories();

  // Filter and sort templates
  const filteredTemplates = React.useMemo(() => {
    let result = searchQuery.trim()
      ? searchTemplates(searchQuery)
      : templates;

    if (selectedCategory !== 'all') {
      result = result.filter(t => t.category === selectedCategory);
    }

    // Sort
    if (sortBy === 'name') {
      result.sort((a, b) => a.name.localeCompare(b.name));
    } else if (sortBy === 'favorite') {
      result.sort((a, b) => (b.isFavorite ? 1 : 0) - (a.isFavorite ? 1 : 0));
    }

    return result;
  }, [templates, searchQuery, selectedCategory, sortBy, searchTemplates]);

  const handleCreateNew = useCallback(() => {
    setFormData({ name: '', description: '', prompt: '', category: 'general' });
    setSelectedTemplate(null);
    setViewMode('create');
  }, []);

  const handleEditTemplate = useCallback((template: PromptTemplate) => {
    setFormData({
      name: template.name,
      description: template.description,
      prompt: template.prompt,
      category: template.category,
    });
    setSelectedTemplate(template);
    setViewMode('edit');
  }, []);

  const handleSaveTemplate = useCallback(() => {
    // Validate
    if (!formData.name.trim()) {
      toast.error('Template name is required');
      return;
    }
    if (!formData.prompt.trim()) {
      toast.error('Template prompt is required');
      return;
    }

    if (viewMode === 'create') {
      createTemplate(
        formData.name,
        formData.description,
        formData.prompt,
        formData.category
      );
      toast.success('Template created successfully');
    } else if (selectedTemplate) {
      updateTemplate(selectedTemplate.id, {
        name: formData.name,
        description: formData.description,
        prompt: formData.prompt,
        category: formData.category,
      });
      toast.success('Template updated successfully');
    }

    setViewMode('list');
    setSelectedTemplate(null);
  }, [formData, viewMode, selectedTemplate, createTemplate, updateTemplate]);

  const handleDeleteTemplate = useCallback((id: string, name: string) => {
    if (confirm(`Delete template "${name}"? This cannot be undone.`)) {
      deleteTemplate(id);
      toast.success('Template deleted');
      setViewMode('list');
      setSelectedTemplate(null);
    }
  }, [deleteTemplate]);

  const handleSelectTemplate = useCallback((template: PromptTemplate) => {
    if (template.variables.length > 0) {
      // Show variable substitution UI
      setSelectedTemplate(template);
      const initialValues: Record<string, string> = {};
      template.variables.forEach(v => initialValues[v] = '');
      setVariableValues(initialValues);
      setShowVariableSubstitution(true);
    } else {
      // No variables, apply template directly
      if (onSelectTemplate) {
        onSelectTemplate(template);
        onOpenChange(false);
        toast.success(`Using template: ${template.name}`);
      }
    }
  }, [onSelectTemplate, onOpenChange]);

  const handleApplyWithVariables = useCallback(() => {
    if (selectedTemplate && onSelectTemplate) {
      const substituted = substituteVariables(selectedTemplate.id, variableValues);
      if (substituted) {
        onSelectTemplate({ ...selectedTemplate, prompt: substituted });
        onOpenChange(false);
        toast.success(`Applied template: ${selectedTemplate.name}`);
        setShowVariableSubstitution(false);
        setSelectedTemplate(null);
        setVariableValues({});
      }
    }
  }, [selectedTemplate, variableValues, onSelectTemplate, onOpenChange, substituteVariables]);

  const handleToggleFavorite = useCallback((e: React.MouseEvent, template: PromptTemplate) => {
    e.stopPropagation();
    toggleFavorite(template.id);
  }, [toggleFavorite]);

  const handleCopyTemplate = useCallback((e: React.MouseEvent, template: PromptTemplate) => {
    e.stopPropagation();
    navigator.clipboard.writeText(template.prompt);
    toast.success('Template copied to clipboard');
  }, []);

  const handleExportTemplates = useCallback(() => {
    const jsonData = exportTemplates();
    const blob = new Blob([jsonData], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `prompt-templates-${Date.now()}.json`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
    toast.success('Templates exported successfully');
  }, [exportTemplates]);

  const handleImportTemplates = useCallback((event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = (e) => {
      const jsonData = e.target?.result as string;
      if (importTemplates(jsonData)) {
        toast.success('Templates imported successfully');
      } else {
        toast.error('Failed to import templates');
      }
    };
    reader.onerror = () => {
      toast.error('Failed to read file');
    };
    reader.readAsText(file);
    // Reset input
    event.target.value = '';
  }, [importTemplates]);

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="max-w-3xl max-h-[80vh] overflow-hidden flex flex-col">
        <DialogHeader className="border-b pb-4">
          <div className="flex items-center gap-2">
            {viewMode !== 'list' && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setViewMode('list')}
                className="mr-2"
              >
                <ChevronLeft className="h-4 w-4" />
              </Button>
            )}
            <div className="flex-1">
              <DialogTitle>
                {viewMode === 'list'
                  ? 'Prompt Templates'
                  : viewMode === 'create'
                  ? 'Create New Template'
                  : 'Edit Template'}
              </DialogTitle>
              <DialogDescription>
                {viewMode === 'list'
                  ? `Manage your prompt templates (${filteredTemplates.length} templates)`
                  : 'Use {{variable}} syntax for substitution'}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto">
          {viewMode === 'list' ? (
            // List View
            <div className="space-y-4 p-4">
              {/* Search and Filter */}
              <div className="space-y-3">
                <div className="relative">
                  <Search className="absolute left-3 top-3 h-4 w-4 text-muted-foreground" />
                  <Input
                    placeholder="Search templates..."
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    className="pl-10"
                  />
                </div>

                <div className="flex gap-2 flex-wrap">
                  <Select value={selectedCategory} onValueChange={setSelectedCategory}>
                    <SelectTrigger className="w-40">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="all">All Categories</SelectItem>
                      {categories.map((cat) => (
                        <SelectItem key={cat} value={cat}>
                          {cat.charAt(0).toUpperCase() + cat.slice(1)}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>

                  <Select value={sortBy} onValueChange={(v) => setSortBy(v as 'recent' | 'name' | 'favorite')}>
                    <SelectTrigger className="w-40">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="recent">Most Recent</SelectItem>
                      <SelectItem value="name">Name (A-Z)</SelectItem>
                      <SelectItem value="favorite">Favorites</SelectItem>
                    </SelectContent>
                  </Select>

                  <div className="flex gap-2 ml-auto">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={handleExportTemplates}
                      title="Export templates"
                    >
                      <Download className="h-4 w-4" />
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      asChild
                      title="Import templates"
                    >
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
              </div>

              {/* Templates Grid */}
              {filteredTemplates.length === 0 ? (
                <Alert variant="default" className="bg-muted">
                  <AlertTriangle className="h-4 w-4" />
                  <AlertDescription>
                    No templates found. {templates.length === 0 && 'Create your first template to get started.'}
                  </AlertDescription>
                </Alert>
              ) : (
                <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                  {filteredTemplates.map((template) => (
                    <Card
                      key={template.id}
                      className="cursor-pointer hover:bg-accent transition-colors"
                      onClick={() => handleSelectTemplate(template)}
                    >
                      <CardHeader className="pb-3">
                        <div className="flex items-start justify-between gap-2">
                          <div className="flex-1">
                            <CardTitle className="text-sm">{template.name}</CardTitle>
                            <p className="text-xs text-muted-foreground mt-1">
                              {template.description}
                            </p>
                          </div>
                          <div className="flex gap-1">
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={(e) => handleToggleFavorite(e, template)}
                              className="h-8 w-8 p-0"
                            >
                              <Star
                                className={`h-4 w-4 ${
                                  template.isFavorite
                                    ? 'fill-yellow-400 text-yellow-400'
                                    : 'text-muted-foreground'
                                }`}
                              />
                            </Button>
                          </div>
                        </div>
                      </CardHeader>
                      <CardContent className="space-y-2">
                        <div className="flex flex-wrap gap-1">
                          <Badge variant="outline" className="text-xs">
                            {template.category}
                          </Badge>
                          {template.variables.length > 0 && (
                            <Badge variant="secondary" className="text-xs">
                              {template.variables.length} variable{template.variables.length !== 1 ? 's' : ''}
                            </Badge>
                          )}
                        </div>

                        <p className="text-xs line-clamp-2 text-muted-foreground">
                          {template.prompt}
                        </p>

                        <div className="flex gap-1 pt-2">
                          <Button
                            variant="outline"
                            size="sm"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleEditTemplate(template);
                            }}
                            className="h-7 text-xs flex-1"
                          >
                            <Edit2 className="h-3 w-3 mr-1" />
                            Edit
                          </Button>
                          <Button
                            variant="outline"
                            size="sm"
                            onClick={(e) => handleCopyTemplate(e, template)}
                            className="h-7 w-7 p-0"
                          >
                            <Copy className="h-3 w-3" />
                          </Button>
                          <Button
                            variant="outline"
                            size="sm"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleDeleteTemplate(template.id, template.name);
                            }}
                            className="h-7 w-7 p-0 text-destructive hover:text-destructive"
                          >
                            <Trash2 className="h-3 w-3" />
                          </Button>
                        </div>
                      </CardContent>
                    </Card>
                  ))}
                </div>
              )}
            </div>
          ) : (
            // Create/Edit View
            <div className="space-y-4 p-4">
              <div className="space-y-2">
                <Label htmlFor="name">Template Name *</Label>
                <Input
                  id="name"
                  placeholder="e.g., Code Review"
                  value={formData.name}
                  onChange={(e) =>
                    setFormData({ ...formData, name: e.target.value })
                  }
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="category">Category</Label>
                <Select value={formData.category} onValueChange={(v) =>
                  setFormData({ ...formData, category: v })
                }>
                  <SelectTrigger id="category">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="general">General</SelectItem>
                    <SelectItem value="engineering">Engineering</SelectItem>
                    <SelectItem value="writing">Writing</SelectItem>
                    <SelectItem value="education">Education</SelectItem>
                    <SelectItem value="analysis">Analysis</SelectItem>
                    <SelectItem value="other">Other</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-2">
                <Label htmlFor="description">Description</Label>
                <Input
                  id="description"
                  placeholder="Brief description of this template"
                  value={formData.description}
                  onChange={(e) =>
                    setFormData({ ...formData, description: e.target.value })
                  }
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="prompt">Prompt Template *</Label>
                <Textarea
                  id="prompt"
                  placeholder="Enter your template. Use {{variable}} syntax for substitution."
                  value={formData.prompt}
                  onChange={(e) =>
                    setFormData({ ...formData, prompt: e.target.value })
                  }
                  rows={8}
                />
                <p className="text-xs text-muted-foreground">
                  Use double braces like {`{{variable}}`} for variable substitution.
                  Example: {`{{code}}`}, {`{{language}}`}
                </p>
              </div>

              {/* Variable Preview */}
              {formData.prompt.includes('{{') && (
                <Alert className="bg-blue-50 border-blue-200">
                  <AlertDescription className="text-sm">
                    <strong>Variables detected:</strong>
                    <div className="flex flex-wrap gap-1 mt-1">
                      {Array.from(
                        /\{\{(\w+)\}\}/g[Symbol.matchAll](formData.prompt)
                      ).map(([, variable], idx) => (
                        <Badge key={idx} variant="secondary" className="text-xs">
                          {variable}
                        </Badge>
                      ))}
                    </div>
                  </AlertDescription>
                </Alert>
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="border-t pt-4 px-4 pb-4 flex justify-between gap-2">
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {viewMode === 'list' ? 'Close' : 'Cancel'}
          </Button>

          <div className="flex gap-2">
            {viewMode === 'list' && (
              <Button onClick={handleCreateNew} className="gap-2">
                <Plus className="h-4 w-4" />
                New Template
              </Button>
            )}
            {viewMode !== 'list' && (
              <Button onClick={handleSaveTemplate} className="gap-2">
                <Check className="h-4 w-4" />
                Save Template
              </Button>
            )}
          </div>
        </div>
        </DialogContent>
      </Dialog>

      {/* Variable Substitution Dialog */}
      {showVariableSubstitution && selectedTemplate && (
        <Dialog open={showVariableSubstitution} onOpenChange={setShowVariableSubstitution}>
          <DialogContent className="max-w-2xl max-h-[80vh] overflow-hidden flex flex-col">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <FileText className="h-5 w-5" />
              Fill in Variables - {selectedTemplate.name}
            </DialogTitle>
            <DialogDescription>
              Complete the variables below to generate your prompt
            </DialogDescription>
          </DialogHeader>

          <div className="flex-1 overflow-y-auto space-y-4 p-4">
            {selectedTemplate.variables.map((variable) => (
              <div key={variable} className="space-y-2">
                <Label htmlFor={`var-${variable}`} className="capitalize">
                  {variable.replace(/_/g, ' ')}
                </Label>
                <Textarea
                  id={`var-${variable}`}
                  placeholder={`Enter ${variable.replace(/_/g, ' ')}...`}
                  value={variableValues[variable] || ''}
                  onChange={(e) =>
                    setVariableValues({ ...variableValues, [variable]: e.target.value })
                  }
                  rows={3}
                />
              </div>
            ))}

            {/* Preview */}
            <div className="space-y-2 pt-4 border-t">
              <Label>Preview</Label>
              <div className="p-3 bg-muted rounded-md text-sm whitespace-pre-wrap font-mono max-h-48 overflow-y-auto">
                {substituteVariables(selectedTemplate.id, variableValues) || selectedTemplate.prompt}
              </div>
            </div>
          </div>

          <div className="border-t pt-4 px-4 pb-4 flex justify-between gap-2">
            <Button
              variant="outline"
              onClick={() => {
                setShowVariableSubstitution(false);
                setSelectedTemplate(null);
                setVariableValues({});
              }}
            >
              Cancel
            </Button>
            <Button
              onClick={handleApplyWithVariables}
              disabled={selectedTemplate.variables.some(v => !variableValues[v]?.trim())}
              className="gap-2"
            >
              <Check className="h-4 w-4" />
              Apply Template
            </Button>
          </div>
          </DialogContent>
        </Dialog>
      )}
    </>
  );
}
