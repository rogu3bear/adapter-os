import { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { useTraining } from '@/hooks/useTraining';
import { PageErrors, usePageErrors } from '@/components/ui/page-error-boundary';
import {
  FileText,
  RefreshCw,
  Eye,
  Zap,
  Settings,
  Layers,
  TrendingUp,
} from 'lucide-react';
import type { TrainingTemplate } from '@/api/training-types';

export function TemplatesTab() {
  const { errors, addError } = usePageErrors();
  const [selectedTemplate, setSelectedTemplate] = useState<TrainingTemplate | null>(null);

  const {
    data: templatesData,
    isLoading,
    error,
    refetch,
  } = useTraining.useTemplates();

  // Handle errors outside of query options (React Query v5 compatibility)
  if (error) {
    addError('fetch-templates', error.message, () => refetch());
  }

  const templates = templatesData || [];

  const formatNumber = (num?: number): string => {
    if (num === undefined || num === null) return '-';
    return num.toLocaleString();
  };

  return (
    <div className="space-y-6">
      {/* Action Bar */}
      <div className="flex items-center justify-between">
        <p className="text-sm text-muted-foreground">
          Pre-configured training templates for common use cases
        </p>
        <Button
          variant="outline"
          size="sm"
          onClick={() => refetch()}
          disabled={isLoading}
        >
          <RefreshCw className={`h-4 w-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
          Refresh
        </Button>
      </div>

      <PageErrors errors={errors} />

      {error && (
        <Card className="border-destructive">
          <CardContent className="pt-6">
            <p className="text-destructive">Failed to load templates: {error.message}</p>
            <Button variant="outline" onClick={() => refetch()} className="mt-2">
              Retry
            </Button>
          </CardContent>
        </Card>
      )}

      {/* Templates Grid */}
      {isLoading && templates.length === 0 ? (
        <div className="text-center py-8 text-muted-foreground">
          <RefreshCw className="h-6 w-6 animate-spin mx-auto mb-2" />
          Loading templates...
        </div>
      ) : templates.length === 0 ? (
        <div className="text-center py-8 text-muted-foreground">
          <FileText className="h-8 w-8 mx-auto mb-2 opacity-50" />
          <p>No training templates found</p>
          <p className="text-sm mt-1">Templates will be available soon</p>
        </div>
      ) : (
        <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
          {templates.map((template) => (
            <Card
              key={template.id}
              className="hover:shadow-lg transition-shadow cursor-pointer"
              onClick={() => setSelectedTemplate(template)}
            >
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <FileText className="h-5 w-5 text-primary" />
                  {template.name}
                </CardTitle>
                {template.description && (
                  <CardDescription>{template.description}</CardDescription>
                )}
              </CardHeader>
              <CardContent>
                <div className="space-y-3">
                  {template.category && (
                    <div className="flex items-center justify-between">
                      <span className="text-sm text-muted-foreground">Category</span>
                      <Badge variant="outline">{template.category}</Badge>
                    </div>
                  )}

                  {template.rank !== undefined && (
                    <div className="flex items-center justify-between text-sm">
                      <span className="text-muted-foreground flex items-center gap-1">
                        <Layers className="h-3 w-3" />
                        Rank
                      </span>
                      <span className="font-medium">{template.rank}</span>
                    </div>
                  )}

                  {template.alpha !== undefined && (
                    <div className="flex items-center justify-between text-sm">
                      <span className="text-muted-foreground flex items-center gap-1">
                        <Settings className="h-3 w-3" />
                        Alpha
                      </span>
                      <span className="font-medium">{template.alpha}</span>
                    </div>
                  )}

                  {template.learning_rate !== undefined && (
                    <div className="flex items-center justify-between text-sm">
                      <span className="text-muted-foreground flex items-center gap-1">
                        <TrendingUp className="h-3 w-3" />
                        Learning Rate
                      </span>
                      <span className="font-mono font-medium">
                        {template.learning_rate.toExponential(2)}
                      </span>
                    </div>
                  )}

                  {template.default_epochs !== undefined && (
                    <div className="flex items-center justify-between text-sm">
                      <span className="text-muted-foreground flex items-center gap-1">
                        <Zap className="h-3 w-3" />
                        Epochs
                      </span>
                      <span className="font-medium">{template.default_epochs}</span>
                    </div>
                  )}

                  {template.default_batch_size !== undefined && (
                    <div className="flex items-center justify-between text-sm">
                      <span className="text-muted-foreground">Batch Size</span>
                      <span className="font-medium">{template.default_batch_size}</span>
                    </div>
                  )}

                  {template.target_modules && template.target_modules.length > 0 && (
                    <div className="pt-2 border-t">
                      <span className="text-xs text-muted-foreground">Target Modules</span>
                      <div className="flex flex-wrap gap-1 mt-1">
                        {template.target_modules.slice(0, 3).map((module) => (
                          <Badge key={module} variant="secondary" className="text-xs">
                            {module}
                          </Badge>
                        ))}
                        {template.target_modules.length > 3 && (
                          <Badge variant="secondary" className="text-xs">
                            +{template.target_modules.length - 3} more
                          </Badge>
                        )}
                      </div>
                    </div>
                  )}

                  <Button
                    variant="outline"
                    size="sm"
                    className="w-full mt-2"
                    onClick={(e) => {
                      e.stopPropagation();
                      setSelectedTemplate(template);
                    }}
                  >
                    <Eye className="h-4 w-4 mr-2" />
                    View Details
                  </Button>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      {/* Template Detail Dialog */}
      <Dialog open={!!selectedTemplate} onOpenChange={() => setSelectedTemplate(null)}>
        <DialogContent className="max-w-3xl max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <FileText className="h-5 w-5" />
              {selectedTemplate?.name}
            </DialogTitle>
          </DialogHeader>
          {selectedTemplate && (
            <div className="space-y-6">
              {selectedTemplate.description && (
                <div>
                  <Label className="text-muted-foreground">Description</Label>
                  <p className="mt-1">{selectedTemplate.description}</p>
                </div>
              )}

              <div className="grid grid-cols-2 gap-4">
                {selectedTemplate.id && (
                  <div>
                    <Label className="text-muted-foreground">Template ID</Label>
                    <p className="font-mono text-sm">{selectedTemplate.id}</p>
                  </div>
                )}

                {selectedTemplate.category && (
                  <div>
                    <Label className="text-muted-foreground">Category</Label>
                    <Badge variant="outline" className="mt-1">
                      {selectedTemplate.category}
                    </Badge>
                  </div>
                )}

                {selectedTemplate.rank !== undefined && (
                  <div>
                    <Label className="text-muted-foreground">LoRA Rank</Label>
                    <p className="font-medium">{selectedTemplate.rank}</p>
                  </div>
                )}

                {selectedTemplate.alpha !== undefined && (
                  <div>
                    <Label className="text-muted-foreground">LoRA Alpha</Label>
                    <p className="font-medium">{selectedTemplate.alpha}</p>
                  </div>
                )}

                {selectedTemplate.learning_rate !== undefined && (
                  <div>
                    <Label className="text-muted-foreground">Learning Rate</Label>
                    <p className="font-mono">{selectedTemplate.learning_rate.toExponential(2)}</p>
                  </div>
                )}

                {selectedTemplate.default_epochs !== undefined && (
                  <div>
                    <Label className="text-muted-foreground">Default Epochs</Label>
                    <p className="font-medium">{selectedTemplate.default_epochs}</p>
                  </div>
                )}

                {selectedTemplate.default_batch_size !== undefined && (
                  <div>
                    <Label className="text-muted-foreground">Default Batch Size</Label>
                    <p className="font-medium">{selectedTemplate.default_batch_size}</p>
                  </div>
                )}

                {selectedTemplate.created_at && (
                  <div>
                    <Label className="text-muted-foreground">Created</Label>
                    <p className="text-sm">
                      {new Date(selectedTemplate.created_at).toLocaleString()}
                    </p>
                  </div>
                )}
              </div>

              {selectedTemplate.target_modules && selectedTemplate.target_modules.length > 0 && (
                <div>
                  <Label className="text-muted-foreground">Target Modules</Label>
                  <div className="flex flex-wrap gap-2 mt-2">
                    {selectedTemplate.target_modules.map((module) => (
                      <Badge key={module} variant="secondary">
                        {module}
                      </Badge>
                    ))}
                  </div>
                </div>
              )}

              {selectedTemplate.config && (
                <div>
                  <Label className="text-muted-foreground">Configuration</Label>
                  <pre className="mt-2 p-4 bg-muted rounded-lg text-sm overflow-x-auto">
                    {JSON.stringify(selectedTemplate.config, null, 2)}
                  </pre>
                </div>
              )}

              <div className="flex justify-end pt-4 border-t">
                <Button onClick={() => setSelectedTemplate(null)}>Close</Button>
              </div>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
