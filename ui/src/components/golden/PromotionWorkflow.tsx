import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { Badge } from '../ui/badge';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '../ui/dialog';
import { Alert, AlertDescription } from '../ui/alert';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Textarea } from '../ui/textarea';
import { BreadcrumbNavigation } from '../BreadcrumbNavigation';
import {
  CheckCircle,
  XCircle,
  Clock,
  AlertTriangle,
  ArrowRight,
  RotateCcw,
  Shield,
  FileCheck,
  Users,
  ChevronRight
} from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '../../api/client';
import { logger, toError } from '../../utils/logger';
import { DensityProvider, useDensity } from '../../contexts/DensityContext';

// Types
interface PromotionStage {
  id: string;
  name: string;
  description: string;
  status: 'pending' | 'in_progress' | 'passed' | 'failed' | 'skipped';
  approver?: string;
  approved_at?: string;
  notes?: string;
  gates: PromotionGate[];
}

interface PromotionGate {
  id: string;
  name: string;
  description: string;
  status: 'pending' | 'passed' | 'failed';
  required: boolean;
  error_message?: string;
  last_checked?: string;
}

interface PromotionWorkflowProps {
  goldenRunId: string;
  onComplete?: () => void;
  onCancel?: () => void;
}

interface ApprovalRequest {
  stage_id: string;
  justification: string;
  target_environment: string;
}

interface RollbackPlan {
  trigger_conditions: string[];
  rollback_steps: string[];
  notification_contacts: string[];
}

// Stage Card Component
function StageCard({ stage, isActive, isCompleted, onApprove, onReject }: {
  stage: PromotionStage;
  isActive: boolean;
  isCompleted: boolean;
  onApprove: (notes: string) => void;
  onReject: (reason: string) => void;
}) {
  const [showApproval, setShowApproval] = useState(false);
  const [notes, setNotes] = useState('');
  const [reason, setReason] = useState('');

  const getStatusIcon = () => {
    switch (stage.status) {
      case 'passed':
        return <CheckCircle className="h-5 w-5 text-green-500" />;
      case 'failed':
        return <XCircle className="h-5 w-5 text-red-500" />;
      case 'in_progress':
        return <Clock className="h-5 w-5 text-blue-500 animate-pulse" />;
      default:
        return <Clock className="h-5 w-5 text-gray-400" />;
    }
  };

  const getStatusBadge = () => {
    const variants: Record<string, 'default' | 'secondary' | 'success' | 'destructive'> = {
      passed: 'success',
      failed: 'destructive',
      in_progress: 'default',
      pending: 'secondary',
      skipped: 'secondary'
    };
    return <Badge variant={variants[stage.status] || 'secondary'}>{stage.status.replace('_', ' ')}</Badge>;
  };

  const allGatesPassed = stage.gates.every(g => !g.required || g.status === 'passed');
  const anyGateFailed = stage.gates.some(g => g.required && g.status === 'failed');

  return (
    <>
      <Card className={`transition-all ${isActive ? 'border-primary shadow-md' : ''} ${isCompleted ? 'opacity-75' : ''}`}>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              {getStatusIcon()}
              <div>
                <CardTitle className="text-lg">{stage.name}</CardTitle>
                <CardDescription>{stage.description}</CardDescription>
              </div>
            </div>
            {getStatusBadge()}
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Gates Status */}
          <div className="space-y-2">
            <h4 className="text-sm font-medium flex items-center gap-2">
              <Shield className="h-4 w-4" />
              Gate Checks ({stage.gates.filter(g => g.status === 'passed').length}/{stage.gates.length})
            </h4>
            <div className="grid gap-2">
              {stage.gates.map(gate => (
                <div key={gate.id} className="flex items-center justify-between p-2 bg-muted rounded-md">
                  <div className="flex items-center gap-2">
                    {gate.status === 'passed' && <CheckCircle className="h-4 w-4 text-green-500" />}
                    {gate.status === 'failed' && <XCircle className="h-4 w-4 text-red-500" />}
                    {gate.status === 'pending' && <Clock className="h-4 w-4 text-gray-400" />}
                    <div>
                      <p className="text-sm font-medium">{gate.name}</p>
                      <p className="text-xs text-muted-foreground">{gate.description}</p>
                      {gate.error_message && (
                        <p className="text-xs text-red-500 mt-1">{gate.error_message}</p>
                      )}
                    </div>
                  </div>
                  {gate.required && <Badge variant="outline" className="text-xs">Required</Badge>}
                </div>
              ))}
            </div>
          </div>

          {/* Approval Info */}
          {stage.approver && (
            <div className="p-3 bg-muted rounded-md">
              <div className="flex items-center gap-2 mb-1">
                <Users className="h-4 w-4" />
                <p className="text-sm font-medium">Approved by {stage.approver}</p>
              </div>
              {stage.approved_at && (
                <p className="text-xs text-muted-foreground">
                  {new Date(stage.approved_at).toLocaleString()}
                </p>
              )}
              {stage.notes && (
                <p className="text-xs mt-2">{stage.notes}</p>
              )}
            </div>
          )}

          {/* Approval Actions */}
          {isActive && stage.status === 'in_progress' && (
            <div className="flex gap-2">
              <Button
                variant="default"
                size="sm"
                disabled={!allGatesPassed}
                onClick={() => setShowApproval(true)}
                className="flex-1"
              >
                <CheckCircle className="h-4 w-4 mr-2" />
                Approve
              </Button>
              <Button
                variant="destructive"
                size="sm"
                onClick={() => {
                  setShowApproval(true);
                }}
                className="flex-1"
              >
                <XCircle className="h-4 w-4 mr-2" />
                Reject
              </Button>
            </div>
          )}

          {anyGateFailed && stage.status !== 'failed' && (
            <Alert>
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>
                Some required gates have failed. Please address the issues before proceeding.
              </AlertDescription>
            </Alert>
          )}
        </CardContent>
      </Card>

      {/* Approval Dialog */}
      <Dialog open={showApproval} onOpenChange={setShowApproval}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {allGatesPassed ? 'Approve Promotion' : 'Reject Promotion'}
            </DialogTitle>
            <DialogDescription>
              {allGatesPassed
                ? `Approve promotion to ${stage.name}. This action will advance the golden run to the next stage.`
                : 'Provide a reason for rejection. The promotion will be blocked.'
              }
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor={allGatesPassed ? 'notes' : 'reason'}>
                {allGatesPassed ? 'Approval Notes (Optional)' : 'Rejection Reason (Required)'}
              </Label>
              <Textarea
                id={allGatesPassed ? 'notes' : 'reason'}
                placeholder={allGatesPassed
                  ? 'Add any notes about this approval...'
                  : 'Explain why this promotion is being rejected...'
                }
                value={allGatesPassed ? notes : reason}
                onChange={(e) => allGatesPassed ? setNotes(e.target.value) : setReason(e.target.value)}
                rows={4}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => {
              setShowApproval(false);
              setNotes('');
              setReason('');
            }}>
              Cancel
            </Button>
            {allGatesPassed ? (
              <Button
                variant="default"
                onClick={() => {
                  onApprove(notes);
                  setShowApproval(false);
                  setNotes('');
                }}
              >
                <CheckCircle className="h-4 w-4 mr-2" />
                Confirm Approval
              </Button>
            ) : (
              <Button
                variant="destructive"
                disabled={!reason.trim()}
                onClick={() => {
                  onReject(reason);
                  setShowApproval(false);
                  setReason('');
                }}
              >
                <XCircle className="h-4 w-4 mr-2" />
                Confirm Rejection
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

// Rollback Plan Editor Component
function RollbackPlanEditor({ plan, onChange }: {
  plan: RollbackPlan;
  onChange: (plan: RollbackPlan) => void;
}) {
  const [triggerCondition, setTriggerCondition] = useState('');
  const [rollbackStep, setRollbackStep] = useState('');
  const [contact, setContact] = useState('');

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-lg flex items-center gap-2">
          <RotateCcw className="h-5 w-5" />
          Rollback Plan
        </CardTitle>
        <CardDescription>
          Define conditions and procedures for emergency rollback
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* Trigger Conditions */}
        <div className="space-y-2">
          <Label>Trigger Conditions</Label>
          <div className="flex gap-2">
            <Input
              placeholder="e.g., Error rate > 5%"
              value={triggerCondition}
              onChange={(e) => setTriggerCondition(e.target.value)}
            />
            <Button
              size="sm"
              onClick={() => {
                if (triggerCondition.trim()) {
                  onChange({
                    ...plan,
                    trigger_conditions: [...plan.trigger_conditions, triggerCondition.trim()]
                  });
                  setTriggerCondition('');
                }
              }}
            >
              Add
            </Button>
          </div>
          <div className="space-y-1">
            {plan.trigger_conditions.map((cond, idx) => (
              <div key={idx} className="flex items-center justify-between p-2 bg-muted rounded-md">
                <span className="text-sm">{cond}</span>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    onChange({
                      ...plan,
                      trigger_conditions: plan.trigger_conditions.filter((_, i) => i !== idx)
                    });
                  }}
                >
                  <XCircle className="h-4 w-4" />
                </Button>
              </div>
            ))}
          </div>
        </div>

        {/* Rollback Steps */}
        <div className="space-y-2">
          <Label>Rollback Steps</Label>
          <div className="flex gap-2">
            <Input
              placeholder="e.g., Revert to previous golden run"
              value={rollbackStep}
              onChange={(e) => setRollbackStep(e.target.value)}
            />
            <Button
              size="sm"
              onClick={() => {
                if (rollbackStep.trim()) {
                  onChange({
                    ...plan,
                    rollback_steps: [...plan.rollback_steps, rollbackStep.trim()]
                  });
                  setRollbackStep('');
                }
              }}
            >
              Add
            </Button>
          </div>
          <div className="space-y-1">
            {plan.rollback_steps.map((step, idx) => (
              <div key={idx} className="flex items-center justify-between p-2 bg-muted rounded-md">
                <div className="flex items-center gap-2">
                  <span className="text-xs font-medium text-muted-foreground">{idx + 1}.</span>
                  <span className="text-sm">{step}</span>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    onChange({
                      ...plan,
                      rollback_steps: plan.rollback_steps.filter((_, i) => i !== idx)
                    });
                  }}
                >
                  <XCircle className="h-4 w-4" />
                </Button>
              </div>
            ))}
          </div>
        </div>

        {/* Notification Contacts */}
        <div className="space-y-2">
          <Label>Notification Contacts</Label>
          <div className="flex gap-2">
            <Input
              placeholder="email@example.com"
              value={contact}
              onChange={(e) => setContact(e.target.value)}
            />
            <Button
              size="sm"
              onClick={() => {
                if (contact.trim()) {
                  onChange({
                    ...plan,
                    notification_contacts: [...plan.notification_contacts, contact.trim()]
                  });
                  setContact('');
                }
              }}
            >
              Add
            </Button>
          </div>
          <div className="space-y-1">
            {plan.notification_contacts.map((cont, idx) => (
              <div key={idx} className="flex items-center justify-between p-2 bg-muted rounded-md">
                <span className="text-sm">{cont}</span>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    onChange({
                      ...plan,
                      notification_contacts: plan.notification_contacts.filter((_, i) => i !== idx)
                    });
                  }}
                >
                  <XCircle className="h-4 w-4" />
                </Button>
              </div>
            ))}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

// Main Component
function PromotionWorkflowInner({ goldenRunId, onComplete, onCancel }: PromotionWorkflowProps) {
  const { density, spacing, textSizes } = useDensity();
  const [stages, setStages] = useState<PromotionStage[]>([]);
  const [currentStageIdx, setCurrentStageIdx] = useState(0);
  const [isLoading, setIsLoading] = useState(true);
  const [showRollbackPlan, setShowRollbackPlan] = useState(false);
  const [rollbackPlan, setRollbackPlan] = useState<RollbackPlan>({
    trigger_conditions: [],
    rollback_steps: [],
    notification_contacts: []
  });

  useEffect(() => {
    loadPromotionStatus();
  }, [goldenRunId]);

  const loadPromotionStatus = async () => {
    setIsLoading(true);
    try {
      // Fetch real promotion status from API
      const response = await apiClient.getGoldenPromotionStatus(goldenRunId);

      // Transform API response to component's stage format
      // Note: If the API returns different structure, adjust mapping accordingly
      const apiStages: PromotionStage[] = response.stages?.map((stage: {
        id: string;
        name: string;
        description?: string;
        status: string;
        approver?: string;
        approved_at?: string;
        notes?: string;
        gates?: Array<{
          id: string;
          name: string;
          description?: string;
          status: string;
          required?: boolean;
        }>;
      }) => ({
        id: stage.id,
        name: stage.name,
        description: stage.description || '',
        status: stage.status as 'pending' | 'in_progress' | 'passed' | 'failed',
        approver: stage.approver,
        approved_at: stage.approved_at,
        notes: stage.notes,
        gates: stage.gates?.map(g => ({
          id: g.id,
          name: g.name,
          description: g.description || '',
          status: g.status as 'pending' | 'passed' | 'failed',
          required: g.required ?? true
        })) || []
      })) || [];

      if (apiStages.length === 0) {
        // No promotion data yet - show empty state
        toast.info('No promotion workflow configured for this golden run');
      }

      setStages(apiStages);
      setCurrentStageIdx(apiStages.findIndex(s => s.status === 'in_progress'));
    } catch (error) {
      logger.error('Failed to load promotion status', {
        component: 'PromotionWorkflow',
        operation: 'loadPromotionStatus',
        goldenRunId
      }, toError(error));
      toast.error('Failed to load promotion status');
    } finally {
      setIsLoading(false);
    }
  };

  const handleApprove = async (stageIdx: number, notes: string) => {
    try {
      // Real API call to approve stage
      await apiClient.approveGoldenPromotion(goldenRunId, stages[stageIdx].id, notes);

      // Reload status from server to get accurate state
      await loadPromotionStatus();
      toast.success(`Stage ${stages[stageIdx].name} approved`);
    } catch (error) {
      logger.error('Failed to approve stage', {
        component: 'PromotionWorkflow',
        operation: 'handleApprove',
        stageIdx
      }, toError(error));
      toast.error('Failed to approve stage');
    }
  };

  const handleReject = async (stageIdx: number, reason: string) => {
    try {
      // Real API call to reject stage
      await apiClient.rejectGoldenPromotion(goldenRunId, stages[stageIdx].id, reason);

      // Reload status from server to get accurate state
      await loadPromotionStatus();
      toast.error(`Stage ${stages[stageIdx].name} rejected`);
    } catch (error) {
      logger.error('Failed to reject stage', {
        component: 'PromotionWorkflow',
        operation: 'handleReject',
        stageIdx
      }, toError(error));
      toast.error('Failed to reject stage');
    }
  };

  const handleExecutePromotion = async () => {
    if (currentStageIdx >= stages.length - 1 && stages[stages.length - 1].status === 'passed') {
      try {
        // Real API call to request promotion to production
        await apiClient.requestGoldenPromotion(goldenRunId, 'production');
        await loadPromotionStatus();

        toast.success('Promotion executed successfully');
        onComplete?.();
      } catch (error) {
        logger.error('Failed to execute promotion', {
          component: 'PromotionWorkflow',
          operation: 'handleExecutePromotion'
        }, toError(error));
        toast.error('Failed to execute promotion');
      }
    }
  };

  const handleRollback = async () => {
    try {
      // Real API call to rollback promotion
      const currentStage = stages[currentStageIdx]?.id || 'production';
      await apiClient.rollbackGoldenPromotion(currentStage);
      await loadPromotionStatus();
      toast.success('Rollback initiated');
    } catch (error) {
      logger.error('Failed to rollback promotion', {
        component: 'PromotionWorkflow',
        operation: 'handleRollback'
      }, toError(error));
      toast.error('Failed to rollback promotion');
    }
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
      </div>
    );
  }

  const allStagesPassed = stages.every(s => s.status === 'passed');
  const canExecute = allStagesPassed && rollbackPlan.rollback_steps.length > 0;

  return (
    <div className={spacing.sectionGap}>
      <BreadcrumbNavigation />

      <div className="flex items-center justify-between mb-6">
        <div>
          <h2 className={textSizes.title}>Promotion Workflow</h2>
          <p className="text-sm text-muted-foreground mt-1">
            Golden Run: {goldenRunId}
          </p>
        </div>
        <div className="flex gap-2">
          {onCancel && (
            <Button variant="outline" onClick={onCancel}>
              Cancel
            </Button>
          )}
          <Button
            variant="outline"
            onClick={() => setShowRollbackPlan(!showRollbackPlan)}
          >
            <FileCheck className="h-4 w-4 mr-2" />
            Rollback Plan
          </Button>
        </div>
      </div>

      {/* Progress Pipeline */}
      <Card className="mb-6">
        <CardContent className="pt-6">
          <div className="flex items-center justify-between">
            {stages.map((stage, idx) => (
              <React.Fragment key={stage.id}>
                <div className="flex flex-col items-center gap-2">
                  <div
                    className={`
                      flex items-center justify-center w-12 h-12 rounded-full border-2 transition-all
                      ${stage.status === 'passed'
                        ? 'bg-green-500 border-green-500 text-white'
                        : stage.status === 'in_progress'
                        ? 'bg-blue-500 border-blue-500 text-white'
                        : stage.status === 'failed'
                        ? 'bg-red-500 border-red-500 text-white'
                        : 'bg-background border-gray-300 text-muted-foreground'
                      }
                    `}
                  >
                    {stage.status === 'passed' && <CheckCircle className="h-6 w-6" />}
                    {stage.status === 'failed' && <XCircle className="h-6 w-6" />}
                    {stage.status === 'in_progress' && <Clock className="h-6 w-6 animate-pulse" />}
                    {stage.status === 'pending' && <span className="text-sm font-medium">{idx + 1}</span>}
                  </div>
                  <div className="text-center">
                    <p className="text-sm font-medium">{stage.name}</p>
                    <Badge variant="outline" className="mt-1 text-xs">
                      {stage.status.replace('_', ' ')}
                    </Badge>
                  </div>
                </div>
                {idx < stages.length - 1 && (
                  <div className="flex-1 mx-4">
                    <div
                      className={`
                        h-1 rounded transition-all
                        ${stage.status === 'passed' ? 'bg-green-500' : 'bg-gray-300'}
                      `}
                    />
                  </div>
                )}
              </React.Fragment>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* Rollback Plan Editor */}
      {showRollbackPlan && (
        <div className="mb-6">
          <RollbackPlanEditor plan={rollbackPlan} onChange={setRollbackPlan} />
        </div>
      )}

      {/* Stages */}
      <div className="space-y-4">
        {stages.map((stage, idx) => (
          <StageCard
            key={stage.id}
            stage={stage}
            isActive={idx === currentStageIdx}
            isCompleted={stage.status === 'passed'}
            onApprove={(notes) => handleApprove(idx, notes)}
            onReject={(reason) => handleReject(idx, reason)}
          />
        ))}
      </div>

      {/* Execute Promotion */}
      {allStagesPassed && (
        <Card className="mt-6 border-green-500">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-green-600">
              <CheckCircle className="h-5 w-5" />
              Ready to Promote
            </CardTitle>
            <CardDescription>
              All stages have been approved. Review the rollback plan and execute promotion.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="flex gap-2">
              <Button
                variant="default"
                className="flex-1"
                disabled={!canExecute}
                onClick={handleExecutePromotion}
              >
                <ArrowRight className="h-4 w-4 mr-2" />
                Execute Promotion
              </Button>
              <Button
                variant="destructive"
                onClick={handleRollback}
              >
                <RotateCcw className="h-4 w-4 mr-2" />
                Rollback
              </Button>
            </div>
            {!canExecute && (
              <Alert className="mt-4">
                <AlertTriangle className="h-4 w-4" />
                <AlertDescription>
                  Please define at least one rollback step before executing the promotion.
                </AlertDescription>
              </Alert>
            )}
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// Outer component with DensityProvider
export function PromotionWorkflow(props: PromotionWorkflowProps) {
  return (
    <DensityProvider pageKey="promotion-workflow">
      <PromotionWorkflowInner {...props} />
    </DensityProvider>
  );
}
