import React, { useState, useEffect } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Textarea } from './ui/textarea';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Checkbox } from './ui/checkbox';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from './ui/accordion';
import { Alert, AlertDescription } from './ui/alert';
import { Badge } from './ui/badge';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './ui/card';
import { FileJson, FileText, AlertTriangle, CheckCircle, Save, X } from 'lucide-react';
<<<<<<< HEAD
// 【ui/src/components/PolicyEditor.tsx§1-45】 - Replace toast notifications with ErrorRecovery patterns
import { ErrorRecoveryTemplates } from './ui/error-recovery';
import apiClient from '../api/client';
import { POLICY_PACKS, getDefaultPolicyConfig, PolicyFieldDefinition } from '../constants/policySchema';
import { PolicyPackConfig } from '../api/types';
import { logger, toError } from '../utils/logger';
=======
import { toast } from 'sonner';
import apiClient from '../api/client';
import { POLICY_PACKS, getDefaultPolicyConfig, PolicyFieldDefinition } from '../constants/policySchema';
import { PolicyPackConfig } from '../api/types';
>>>>>>> integration-branch

interface PolicyEditorProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  cpid?: string;
  existingPolicy?: string;
  onSave: () => void;
}

export function PolicyEditor({
  open,
  onOpenChange,
  cpid: initialCpid,
  existingPolicy,
  onSave,
}: PolicyEditorProps) {
  const [mode, setMode] = useState<'form' | 'json'>('form');
  const [cpid, setCpid] = useState(initialCpid || '');
  const [policyConfig, setPolicyConfig] = useState<Record<string, any>>({});
  const [jsonContent, setJsonContent] = useState('');
  const [validationErrors, setValidationErrors] = useState<string[]>([]);
  const [isValidating, setIsValidating] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
<<<<<<< HEAD
  const [editorError, setEditorError] = useState<Error | null>(null);
  const [statusMessage, setStatusMessage] = useState<{ message: string; variant: 'success' | 'warning' | 'info' } | null>(null);

  useEffect(() => {
    if (open) {
      setEditorError(null);
      setStatusMessage(null);
=======

  useEffect(() => {
    if (open) {
>>>>>>> integration-branch
      if (existingPolicy) {
        try {
          const parsed = JSON.parse(existingPolicy);
          setPolicyConfig(parsed);
          setJsonContent(JSON.stringify(parsed, null, 2));
        } catch (err) {
<<<<<<< HEAD
          logger.error('Failed to parse existing policy JSON', {
            component: 'PolicyEditor',
            operation: 'parseExistingPolicy',
            cpid: initialCpid,
          }, toError(err));
=======
          console.error('Failed to parse existing policy:', err);
>>>>>>> integration-branch
          const defaultConfig = getDefaultPolicyConfig();
          setPolicyConfig(defaultConfig);
          setJsonContent(JSON.stringify(defaultConfig, null, 2));
        }
      } else {
        const defaultConfig = getDefaultPolicyConfig();
        setPolicyConfig(defaultConfig);
        setJsonContent(JSON.stringify(defaultConfig, null, 2));
      }
    }
<<<<<<< HEAD
  }, [open, existingPolicy, initialCpid]);
=======
  }, [open, existingPolicy]);
>>>>>>> integration-branch

  const updatePolicyField = (packId: string, fieldName: string, value: any) => {
    setPolicyConfig((prev) => ({
      ...prev,
      packs: {
        ...prev.packs,
        [packId]: {
          ...prev.packs?.[packId],
          [fieldName]: value,
        },
      },
    }));
  };

  const handleModeSwitch = (newMode: 'form' | 'json') => {
    if (newMode === 'json' && mode === 'form') {
      // Switching to JSON mode - convert current form state to JSON
      setJsonContent(JSON.stringify(policyConfig, null, 2));
    } else if (newMode === 'form' && mode === 'json') {
      // Switching to form mode - parse JSON
      try {
        const parsed = JSON.parse(jsonContent);
        setPolicyConfig(parsed);
        setValidationErrors([]);
<<<<<<< HEAD
        setStatusMessage(null);
      } catch (err) {
        setStatusMessage({
          message: 'Invalid JSON. Please fix the JSON before switching to form mode.',
          variant: 'warning'
        });
=======
      } catch (err) {
        toast.error('Invalid JSON. Please fix the JSON before switching to form mode.');
>>>>>>> integration-branch
        return;
      }
    }
    setMode(newMode);
  };

  const handleValidate = async () => {
    setIsValidating(true);
    setValidationErrors([]);
<<<<<<< HEAD
    setEditorError(null);
    setStatusMessage(null);
=======
>>>>>>> integration-branch

    try {
      const content = mode === 'json' ? jsonContent : JSON.stringify(policyConfig);
      const result = await apiClient.validatePolicy({ policy_json: content });

      if (result.valid) {
<<<<<<< HEAD
        setValidationErrors([]);
        setStatusMessage({ message: 'Policy is valid', variant: 'success' });
      } else {
        setValidationErrors(result.errors || []);
        setStatusMessage({
          message: `Policy validation failed: ${result.errors?.length || 0} issues found`,
          variant: 'warning'
        });
=======
        toast.success('Policy is valid');
        setValidationErrors([]);
      } else {
        setValidationErrors(result.errors || []);
        toast.error(`Policy validation failed: ${result.errors?.length} errors found`);
>>>>>>> integration-branch
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Validation failed';
      setValidationErrors([errorMessage]);
<<<<<<< HEAD
      const error = err instanceof Error ? err : new Error('Validation failed');
      setEditorError(error);
=======
      toast.error(errorMessage);
>>>>>>> integration-branch
    } finally {
      setIsValidating(false);
    }
  };

  const handleSave = async () => {
<<<<<<< HEAD
    setEditorError(null);
    setStatusMessage(null);

    if (!cpid.trim()) {
      const message = 'CPID is required';
      setValidationErrors([message]);
      setStatusMessage({ message, variant: 'warning' });
=======
    if (!cpid.trim()) {
      toast.error('CPID is required');
>>>>>>> integration-branch
      return;
    }

    setIsSaving(true);

    try {
      const content = mode === 'json' ? jsonContent : JSON.stringify(policyConfig);
      
      // Validate first
      const validation = await apiClient.validatePolicy({ policy_json: content });
      if (!validation.valid) {
        setValidationErrors(validation.errors || []);
<<<<<<< HEAD
        setStatusMessage({
          message: 'Policy validation failed. Please fix errors before saving.',
          variant: 'warning'
        });
=======
        toast.error('Policy validation failed. Please fix errors before saving.');
>>>>>>> integration-branch
        setIsSaving(false);
        return;
      }

      // Save policy
      await apiClient.createPolicy(cpid, content);
<<<<<<< HEAD
=======
      toast.success(`Policy ${cpid} saved successfully`);
>>>>>>> integration-branch
      onSave();
      onOpenChange(false);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to save policy';
<<<<<<< HEAD
      const error = err instanceof Error ? err : new Error('Failed to save policy');
      setEditorError(error);
      setStatusMessage(null);
=======
      toast.error(errorMessage);
>>>>>>> integration-branch
    } finally {
      setIsSaving(false);
    }
  };

  const renderField = (packId: string, field: PolicyFieldDefinition) => {
    const value = policyConfig.packs?.[packId]?.[field.name];

    switch (field.type) {
      case 'boolean':
        return (
          <div className="flex items-center space-x-2">
            <Checkbox
              id={`${packId}-${field.name}`}
              checked={value || false}
              onCheckedChange={(checked) => updatePolicyField(packId, field.name, checked)}
            />
            <Label htmlFor={`${packId}-${field.name}`} className="text-sm">
              {field.label}
            </Label>
          </div>
        );

      case 'number':
        return (
          <div className="space-y-1">
            <Label htmlFor={`${packId}-${field.name}`} className="text-sm">
              {field.label}
            </Label>
            <Input
              id={`${packId}-${field.name}`}
              type="number"
              value={value || field.default || ''}
              onChange={(e) => updatePolicyField(packId, field.name, parseFloat(e.target.value) || field.default)}
              min={field.min}
              max={field.max}
            />
          </div>
        );

      case 'string':
        return (
          <div className="space-y-1">
            <Label htmlFor={`${packId}-${field.name}`} className="text-sm">
              {field.label}
            </Label>
            <Input
              id={`${packId}-${field.name}`}
              value={value || field.default || ''}
              onChange={(e) => updatePolicyField(packId, field.name, e.target.value)}
            />
          </div>
        );

      case 'enum':
        return (
          <div className="space-y-1">
            <Label htmlFor={`${packId}-${field.name}`} className="text-sm">
              {field.label}
            </Label>
            <Select
              value={value || field.default}
              onValueChange={(val) => updatePolicyField(packId, field.name, val)}
            >
              <SelectTrigger id={`${packId}-${field.name}`}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {field.options?.map((option) => (
                  <SelectItem key={option} value={option}>
                    {option}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        );

      case 'array':
        return (
          <div className="space-y-1">
            <Label htmlFor={`${packId}-${field.name}`} className="text-sm">
              {field.label}
            </Label>
            <Input
              id={`${packId}-${field.name}`}
              value={Array.isArray(value) ? value.join(', ') : ''}
              onChange={(e) => 
                updatePolicyField(
                  packId, 
                  field.name, 
                  e.target.value.split(',').map((s) => s.trim()).filter(Boolean)
                )
              }
              placeholder="Comma-separated values"
            />
          </div>
        );

      case 'object':
        return (
          <div className="space-y-1">
            <Label htmlFor={`${packId}-${field.name}`} className="text-sm">
              {field.label}
            </Label>
            <Textarea
              id={`${packId}-${field.name}`}
              value={typeof value === 'object' ? JSON.stringify(value, null, 2) : '{}'}
              onChange={(e) => {
                try {
                  const parsed = JSON.parse(e.target.value);
                  updatePolicyField(packId, field.name, parsed);
                } catch {
                  // Invalid JSON, ignore
                }
              }}
              rows={3}
              className="font-mono text-xs"
            />
          </div>
        );

      default:
        return null;
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Policy Editor</DialogTitle>
        </DialogHeader>

<<<<<<< HEAD
        <div className="space-y-3">
          {editorError && ErrorRecoveryTemplates.genericError(
            editorError,
            () => setEditorError(null)
          )}

          {statusMessage && (
            <Alert
              className={
                statusMessage.variant === 'success'
                  ? 'border-green-200 bg-green-50'
                  : statusMessage.variant === 'warning'
                    ? 'border-amber-200 bg-amber-50'
                    : 'border-blue-200 bg-blue-50'
              }
            >
              {statusMessage.variant === 'success' ? (
                <CheckCircle className="h-4 w-4 text-green-600" />
              ) : (
                <AlertTriangle className={`h-4 w-4 ${statusMessage.variant === 'warning' ? 'text-amber-600' : 'text-blue-600'}`} />
              )}
              <AlertDescription
                className={
                  statusMessage.variant === 'success'
                    ? 'text-green-700'
                    : statusMessage.variant === 'warning'
                      ? 'text-amber-700'
                      : 'text-blue-700'
                }
              >
                {statusMessage.message}
              </AlertDescription>
            </Alert>
          )}
        </div>

=======
>>>>>>> integration-branch
        <div className="space-y-4">
          {/* CPID Input */}
          <div className="space-y-2">
            <Label htmlFor="cpid">CPID (Control Plane ID)</Label>
            <Input
              id="cpid"
              placeholder="cp-2024-001"
              value={cpid}
              onChange={(e) => setCpid(e.target.value)}
              disabled={!!initialCpid}
            />
          </div>

          {/* Mode Tabs */}
          <Tabs value={mode} onValueChange={(v) => handleModeSwitch(v as 'form' | 'json')}>
            <TabsList className="grid w-full grid-cols-2">
              <TabsTrigger value="form">
                <FileText className="h-4 w-4 mr-2" />
                Form Editor
              </TabsTrigger>
              <TabsTrigger value="json">
                <FileJson className="h-4 w-4 mr-2" />
                JSON Editor
              </TabsTrigger>
            </TabsList>

            {/* Form Mode */}
            <TabsContent value="form" className="space-y-4">
              <Accordion type="multiple" className="w-full">
                {POLICY_PACKS.map((pack) => (
                  <AccordionItem key={pack.id} value={pack.id}>
                    <AccordionTrigger>
                      <div className="flex items-center gap-2">
                        <span className="font-medium">{pack.name}</span>
                        <Badge variant="outline" className="text-xs">
                          {pack.fields.length} fields
                        </Badge>
                      </div>
                    </AccordionTrigger>
                    <AccordionContent>
                      <Card>
                        <CardHeader>
                          <CardDescription>{pack.description}</CardDescription>
                        </CardHeader>
                        <CardContent className="space-y-4">
                          {pack.fields.map((field) => (
                            <div key={field.name}>
                              {renderField(pack.id, field)}
                              {field.description && (
                                <p className="text-xs text-muted-foreground mt-1">
                                  {field.description}
                                </p>
                              )}
                            </div>
                          ))}
                        </CardContent>
                      </Card>
                    </AccordionContent>
                  </AccordionItem>
                ))}
              </Accordion>
            </TabsContent>

            {/* JSON Mode */}
            <TabsContent value="json" className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="json-editor">Policy JSON</Label>
                <Textarea
                  id="json-editor"
                  value={jsonContent}
                  onChange={(e) => setJsonContent(e.target.value)}
                  rows={20}
                  className="font-mono text-sm"
                  placeholder="Enter policy JSON..."
                />
              </div>
            </TabsContent>
          </Tabs>

          {/* Validation Errors */}
          {validationErrors.length > 0 && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>
                <div className="space-y-1">
                  <p className="font-medium">Validation Errors:</p>
                  <ul className="list-disc list-inside">
                    {validationErrors.map((error, idx) => (
                      <li key={idx} className="text-sm">{error}</li>
                    ))}
                  </ul>
                </div>
              </AlertDescription>
            </Alert>
          )}
        </div>

        <DialogFooter className="flex justify-between">
          <div className="flex gap-2">
            <Button variant="outline" onClick={handleValidate} disabled={isValidating}>
              {isValidating ? 'Validating...' : 'Validate'}
            </Button>
          </div>
          <div className="flex gap-2">
            <Button variant="outline" onClick={() => onOpenChange(false)} disabled={isSaving}>
              <X className="h-4 w-4 mr-2" />
              Cancel
            </Button>
            <Button onClick={handleSave} disabled={isSaving || !cpid.trim()}>
              <Save className="h-4 w-4 mr-2" />
              {isSaving ? 'Saving...' : 'Save Policy'}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
<<<<<<< HEAD
=======


>>>>>>> integration-branch
