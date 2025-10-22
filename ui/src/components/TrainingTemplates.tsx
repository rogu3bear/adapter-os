import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Textarea } from './ui/textarea';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from './ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { 
  Code, 
  Layers, 
  GitBranch, 
  Clock, 
  Plus, 
  Edit, 
  Trash2, 
  Copy, 
  Star,
  Download,
  Upload,
  Settings,
  Target,
  Zap,
  CheckCircle,
  AlertTriangle
} from 'lucide-react';
import apiClient from '../api/client';
import { TrainingTemplate, TrainingConfig } from '../api/types';
import { toast } from 'sonner';
import { logger, toError } from '../utils/logger';

interface TrainingTemplatesProps {
  onTemplateSelect: (template: TrainingTemplate) => void;
  onCreateTemplate?: (template: Omit<TrainingTemplate, 'id'>) => void;
}

export function TrainingTemplates({ onTemplateSelect, onCreateTemplate }: TrainingTemplatesProps) {
  const [templates, setTemplates] = useState<TrainingTemplate[]>([]);
  const [loading, setLoading] = useState(true);
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [selectedCategory, setSelectedCategory] = useState<string>('all');
  const [searchQuery, setSearchQuery] = useState('');

  // Mock data removed - using real API data

  useEffect(() => {
    const fetchTemplates = async () => {
      try {
        const templatesData = await apiClient.listTrainingTemplates();
        setTemplates(templatesData);
      } catch (err) {
        logger.error('Failed to fetch training templates', {
          component: 'TrainingTemplates',
          operation: 'listTrainingTemplates',
        }, toError(err));
        toast.error(err instanceof Error ? err.message : 'Failed to load training templates');
      } finally {
        setLoading(false);
      }
    };
    fetchTemplates();
  }, []);

  const getCategoryIcon = (category: string) => {
    switch (category) {
      case 'code': return <Code className="h-4 w-4" />;
      case 'framework': return <Layers className="h-4 w-4" />;
      case 'codebase': return <GitBranch className="h-4 w-4" />;
      case 'ephemeral': return <Clock className="h-4 w-4" />;
      default: return <Code className="h-4 w-4" />;
    }
  };

  const getCategoryColor = (category: string) => {
    switch (category) {
      case 'code': return 'bg-blue-100 text-blue-800';
      case 'framework': return 'bg-green-100 text-green-800';
      case 'codebase': return 'bg-purple-100 text-purple-800';
      case 'ephemeral': return 'bg-orange-100 text-orange-800';
      default: return 'bg-gray-100 text-gray-800';
    }
  };

  const filteredTemplates = templates.filter(template => {
    const matchesCategory = selectedCategory === 'all' || template.category === selectedCategory;
    const matchesSearch = template.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
                         template.description.toLowerCase().includes(searchQuery.toLowerCase());
    return matchesCategory && matchesSearch;
  });

  const handleUseTemplate = (template: TrainingTemplate) => {
    onTemplateSelect(template);
  };

  const handleCreateTemplate = (templateData: Omit<TrainingTemplate, 'id'>) => {
    if (onCreateTemplate) {
      onCreateTemplate(templateData);
    }
    setIsCreateDialogOpen(false);
  };

  if (loading) {
    return <div className="text-center p-8">Loading templates...</div>;
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-semibold">Training Templates</h2>
          <p className="text-muted-foreground">
            Pre-configured training templates for different use cases
          </p>
        </div>
        <Dialog open={isCreateDialogOpen} onOpenChange={setIsCreateDialogOpen}>
          <DialogTrigger asChild>
            <Button>
              <Plus className="mr-2 h-4 w-4" />
              Create Template
            </Button>
          </DialogTrigger>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>Create Training Template</DialogTitle>
            </DialogHeader>
            <CreateTemplateForm onSubmit={handleCreateTemplate} />
          </DialogContent>
        </Dialog>
      </div>

      {/* Filters */}
      <div className="flex items-center space-x-4">
        <div className="flex-1">
          <Input
            placeholder="Search templates..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
        </div>
        <Select value={selectedCategory} onValueChange={setSelectedCategory}>
          <SelectTrigger className="w-48">
            <SelectValue placeholder="Filter by category" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All Categories</SelectItem>
            <SelectItem value="code">Code</SelectItem>
            <SelectItem value="framework">Framework</SelectItem>
            <SelectItem value="codebase">Codebase</SelectItem>
            <SelectItem value="ephemeral">Ephemeral</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {/* Templates Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {filteredTemplates.map((template) => (
          <Card key={template.id} className="cursor-pointer hover:shadow-md transition-shadow">
            <CardHeader>
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  {getCategoryIcon(template.category)}
                  <CardTitle className="text-lg">{template.name}</CardTitle>
                </div>
                <Badge className={getCategoryColor(template.category)}>
                  {template.category}
                </Badge>
              </div>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-muted-foreground mb-4">
                {template.description}
              </p>
              
              <div className="space-y-2 text-sm">
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Rank:</span>
                  <span className="font-medium">{template.rank}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Alpha:</span>
                  <span className="font-medium">{template.alpha}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Epochs:</span>
                  <span className="font-medium">{template.epochs}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Learning Rate:</span>
                  <span className="font-medium">{template.learning_rate}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Batch Size:</span>
                  <span className="font-medium">{template.batch_size}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Targets:</span>
                  <span className="font-medium">{template.targets.length}</span>
                </div>
              </div>

              <div className="flex space-x-2 mt-4">
                <Button 
                  className="flex-1" 
                  size="sm"
                  onClick={() => handleUseTemplate(template)}
                >
                  <Target className="mr-1 h-3 w-3" />
                  Use Template
                </Button>
                <Button variant="outline" size="sm">
                  <Copy className="h-3 w-3" />
                </Button>
                <Button variant="outline" size="sm">
                  <Edit className="h-3 w-3" />
                </Button>
              </div>
            </CardContent>
          </Card>
        ))}
      </div>

      {filteredTemplates.length === 0 && (
        <div className="text-center py-12">
          <AlertTriangle className="h-12 w-12 text-muted-foreground mx-auto mb-4" />
          <h3 className="text-lg font-medium mb-2">No templates found</h3>
          <p className="text-muted-foreground mb-4">
            Try adjusting your search or create a new template.
          </p>
          <Button onClick={() => setIsCreateDialogOpen(true)}>
            <Plus className="mr-2 h-4 w-4" />
            Create Template
          </Button>
        </div>
      )}
    </div>
  );
}

// Create Template Form Component
function CreateTemplateForm({ onSubmit }: { onSubmit: (template: Omit<TrainingTemplate, 'id'>) => void }) {
  const [formData, setFormData] = useState({
    name: '',
    description: '',
    category: 'code' as const,
    rank: 16,
    alpha: 32,
    epochs: 3,
    learning_rate: 0.001,
    batch_size: 32,
    targets: ['q_proj', 'k_proj', 'v_proj', 'o_proj', 'gate_proj', 'up_proj', 'down_proj']
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    onSubmit(formData);
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      <div>
        <Label htmlFor="name">Template Name</Label>
        <Input
          id="name"
          value={formData.name}
          onChange={(e) => setFormData({...formData, name: e.target.value})}
          placeholder="My Custom Template"
          required
        />
      </div>

      <div>
        <Label htmlFor="description">Description</Label>
        <Textarea
          id="description"
          value={formData.description}
          onChange={(e) => setFormData({...formData, description: e.target.value})}
          placeholder="Describe what this template is for..."
          required
        />
      </div>

      <div>
        <Label htmlFor="category">Category</Label>
        <Select value={formData.category} onValueChange={(value: any) => setFormData({...formData, category: value})}>
          <SelectTrigger>
            <SelectValue placeholder="Select category" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="code">Code</SelectItem>
            <SelectItem value="framework">Framework</SelectItem>
            <SelectItem value="codebase">Codebase</SelectItem>
            <SelectItem value="ephemeral">Ephemeral</SelectItem>
          </SelectContent>
        </Select>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div>
          <Label htmlFor="rank">Rank</Label>
          <Input
            id="rank"
            type="number"
            value={formData.rank}
            onChange={(e) => setFormData({...formData, rank: parseInt(e.target.value)})}
            required
          />
        </div>
        <div>
          <Label htmlFor="alpha">Alpha</Label>
          <Input
            id="alpha"
            type="number"
            value={formData.alpha}
            onChange={(e) => setFormData({...formData, alpha: parseInt(e.target.value)})}
            required
          />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div>
          <Label htmlFor="epochs">Epochs</Label>
          <Input
            id="epochs"
            type="number"
            value={formData.epochs}
            onChange={(e) => setFormData({...formData, epochs: parseInt(e.target.value)})}
            required
          />
        </div>
        <div>
          <Label htmlFor="learning_rate">Learning Rate</Label>
          <Input
            id="learning_rate"
            type="number"
            step="0.0001"
            value={formData.learning_rate}
            onChange={(e) => setFormData({...formData, learning_rate: parseFloat(e.target.value)})}
            required
          />
        </div>
      </div>

      <div>
        <Label htmlFor="batch_size">Batch Size</Label>
        <Input
          id="batch_size"
          type="number"
          value={formData.batch_size}
          onChange={(e) => setFormData({...formData, batch_size: parseInt(e.target.value)})}
          required
        />
      </div>

      <div className="flex justify-end space-x-2">
        <Button type="button" variant="outline">
          Cancel
        </Button>
        <Button type="submit">
          Create Template
        </Button>
      </div>
    </form>
  );
}
