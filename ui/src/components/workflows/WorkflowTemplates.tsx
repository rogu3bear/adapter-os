// WorkflowTemplates component - Template selector and browser

import React, { useState, useMemo } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { Badge } from '../ui/badge';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import {
  Zap,
  Rocket,
  FlaskConical,
  GitCompare,
  Layers,
  Wrench,
  Search,
  Clock,
  ChevronRight,
  Star,
  Filter,
} from 'lucide-react';
import { WORKFLOW_TEMPLATES, getTemplatesByCategory, searchTemplates } from './templates';
import { WorkflowTemplate, WorkflowCategory } from './types';

interface WorkflowTemplatesProps {
  onSelectTemplate: (template: WorkflowTemplate) => void;
  onCancel?: () => void;
}

const CATEGORY_ICONS: Record<WorkflowCategory, React.ComponentType<any>> = {
  training: Zap,
  deployment: Rocket,
  experimental: FlaskConical,
  comparison: GitCompare,
  stack: Layers,
  maintenance: Wrench,
};

const CATEGORY_LABELS: Record<WorkflowCategory, string> = {
  training: 'Training',
  deployment: 'Deployment',
  experimental: 'Experimental',
  comparison: 'Comparison',
  stack: 'Stack Management',
  maintenance: 'Maintenance',
};

const DIFFICULTY_COLORS = {
  beginner: 'bg-green-500',
  intermediate: 'bg-yellow-500',
  advanced: 'bg-red-500',
};

export function WorkflowTemplates({ onSelectTemplate, onCancel }: WorkflowTemplatesProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedCategory, setSelectedCategory] = useState<string>('all');
  const [selectedDifficulty, setSelectedDifficulty] = useState<string>('all');
  const [favorites, setFavorites] = useState<Set<string>>(new Set());

  // Filter and search templates
  const filteredTemplates = useMemo(() => {
    let templates = WORKFLOW_TEMPLATES;

    // Apply search
    if (searchQuery.trim()) {
      templates = searchTemplates(searchQuery);
    }

    // Apply category filter
    if (selectedCategory !== 'all') {
      templates = templates.filter((t) => t.category === selectedCategory);
    }

    // Apply difficulty filter
    if (selectedDifficulty !== 'all') {
      templates = templates.filter((t) => t.difficulty === selectedDifficulty);
    }

    return templates;
  }, [searchQuery, selectedCategory, selectedDifficulty]);

  const toggleFavorite = (templateId: string) => {
    setFavorites((prev) => {
      const newFavorites = new Set(prev);
      if (newFavorites.has(templateId)) {
        newFavorites.delete(templateId);
      } else {
        newFavorites.add(templateId);
      }
      return newFavorites;
    });
  };

  const handleSelectTemplate = (template: WorkflowTemplate) => {
    onSelectTemplate(template);
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h2 className="text-3xl font-bold tracking-tight">Workflow Templates</h2>
        <p className="text-muted-foreground mt-2">
          Choose a pre-configured workflow to streamline common tasks
        </p>
      </div>

      {/* Search and Filters */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div className="space-y-2">
          <Label htmlFor="search">Search Templates</Label>
          <div className="relative">
            <Search className="absolute left-2 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input
              id="search"
              placeholder="Search by name, description, or tag..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-8"
            />
          </div>
        </div>

        <div className="space-y-2">
          <Label htmlFor="category">Category</Label>
          <Select value={selectedCategory} onValueChange={setSelectedCategory}>
            <SelectTrigger id="category">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All Categories</SelectItem>
              {Object.entries(CATEGORY_LABELS).map(([key, label]) => (
                <SelectItem key={key} value={key}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="space-y-2">
          <Label htmlFor="difficulty">Difficulty</Label>
          <Select value={selectedDifficulty} onValueChange={setSelectedDifficulty}>
            <SelectTrigger id="difficulty">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All Levels</SelectItem>
              <SelectItem value="beginner">Beginner</SelectItem>
              <SelectItem value="intermediate">Intermediate</SelectItem>
              <SelectItem value="advanced">Advanced</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>

      {/* Results Count */}
      <div className="flex items-center justify-between">
        <p className="text-sm text-muted-foreground">
          Showing {filteredTemplates.length} of {WORKFLOW_TEMPLATES.length} templates
        </p>
        {searchQuery && (
          <Button variant="ghost" size="sm" onClick={() => setSearchQuery('')}>
            Clear Search
          </Button>
        )}
      </div>

      {/* Template Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {filteredTemplates.map((template) => {
          const Icon = CATEGORY_ICONS[template.category];
          const isFavorite = favorites.has(template.id);

          return (
            <Card
              key={template.id}
              className="hover:border-primary transition-all cursor-pointer relative group"
              onClick={() => handleSelectTemplate(template)}
            >
              {/* Favorite Star */}
              <button
                className="absolute top-3 right-3 p-1 rounded-full hover:bg-muted z-10"
                onClick={(e) => {
                  e.stopPropagation();
                  toggleFavorite(template.id);
                }}
              >
                <Star
                  className={`h-4 w-4 ${
                    isFavorite ? 'fill-yellow-500 text-yellow-500' : 'text-muted-foreground'
                  }`}
                />
              </button>

              <CardHeader>
                <div className="flex items-start gap-3">
                  <div className="p-2 rounded-lg bg-primary/10">
                    <Icon className="h-5 w-5 text-primary" />
                  </div>
                  <div className="flex-1">
                    <CardTitle className="text-lg">{template.name}</CardTitle>
                    <CardDescription className="mt-1">{template.description}</CardDescription>
                  </div>
                </div>
              </CardHeader>

              <CardContent className="space-y-3">
                {/* Meta Information */}
                <div className="flex items-center gap-2 text-sm text-muted-foreground">
                  <Clock className="h-3 w-3" />
                  <span>{template.estimatedDuration}</span>
                  <span>•</span>
                  <span>{template.steps.length} steps</span>
                </div>

                {/* Difficulty Badge */}
                <div className="flex items-center gap-2">
                  <div
                    className={`w-2 h-2 rounded-full ${DIFFICULTY_COLORS[template.difficulty]}`}
                  />
                  <span className="text-xs font-medium capitalize">{template.difficulty}</span>
                </div>

                {/* Tags */}
                <div className="flex flex-wrap gap-1">
                  {template.tags.slice(0, 3).map((tag) => (
                    <Badge key={tag} variant="secondary" className="text-xs">
                      {tag}
                    </Badge>
                  ))}
                  {template.tags.length > 3 && (
                    <Badge variant="secondary" className="text-xs">
                      +{template.tags.length - 3}
                    </Badge>
                  )}
                </div>

                {/* Select Button */}
                <Button className="w-full group-hover:bg-primary group-hover:text-primary-foreground">
                  Start Workflow
                  <ChevronRight className="ml-2 h-4 w-4" />
                </Button>
              </CardContent>
            </Card>
          );
        })}
      </div>

      {/* No Results */}
      {filteredTemplates.length === 0 && (
        <Card className="p-8">
          <div className="text-center space-y-3">
            <Filter className="h-12 w-12 text-muted-foreground mx-auto" />
            <h3 className="text-lg font-semibold">No templates found</h3>
            <p className="text-sm text-muted-foreground">
              Try adjusting your search criteria or filters
            </p>
            <Button
              variant="outline"
              onClick={() => {
                setSearchQuery('');
                setSelectedCategory('all');
                setSelectedDifficulty('all');
              }}
            >
              Reset Filters
            </Button>
          </div>
        </Card>
      )}

      {/* Actions */}
      {onCancel && (
        <div className="flex justify-end">
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
        </div>
      )}
    </div>
  );
}
