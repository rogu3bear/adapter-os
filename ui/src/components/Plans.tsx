import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';
import { Label } from './ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Alert, AlertDescription } from './ui/alert';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';
import { 
  FileText, 
  Plus, 
  Zap, 
  MoreHorizontal, 
  RefreshCw, 
  Download, 
  GitCompare,
  Undo2
} from 'lucide-react';
import apiClient from '../api/client';
import { Plan, User, PlanComparisonResponse } from '../api/types';
import { toast } from 'sonner';

interface PlansProps {
  user: User;
  selectedTenant: string;
}

export function Plans({ user, selectedTenant }: PlansProps) {
  const [plans, setPlans] = useState<Plan[]>([]);
  const [loading, setLoading] = useState(true);
  const [showCompareModal, setShowCompareModal] = useState(false);
  const [selectedPlan1, setSelectedPlan1] = useState('');
  const [selectedPlan2, setSelectedPlan2] = useState('');
  const [compareResult, setCompareResult] = useState<PlanComparisonResponse | null>(null);
  const [showRollbackModal, setShowRollbackModal] = useState(false);

  const fetchPlans = async () => {
    setLoading(true);
    try {
      const data = await apiClient.listPlans();
      setPlans(data);
    } catch (err) {
      console.error('Failed to fetch plans:', err);
      toast.error('Failed to load plans');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchPlans();
  }, []);

  const handleRebuild = async (planId: string) => {
    try {
      await apiClient.rebuildPlan(planId);
      toast.success('Plan rebuild initiated');
      fetchPlans();
    } catch (err) {
      console.error('Failed to rebuild plan:', err);
      toast.error('Failed to rebuild plan');
    }
  };

  const handleExportManifest = async (planId: string) => {
    try {
      const blob = await apiClient.exportPlanManifest(planId);
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `plan-${planId}-manifest.json`;
      a.click();
      URL.revokeObjectURL(url);
      toast.success('Manifest downloaded');
    } catch (err) {
      console.error('Failed to export manifest:', err);
      toast.error('Failed to export manifest');
    }
  };

  const handleComparePlans = async () => {
    if (!selectedPlan1 || !selectedPlan2) {
      toast.error('Please select two plans to compare');
      return;
    }
    if (selectedPlan1 === selectedPlan2) {
      toast.error('Please select different plans');
      return;
    }
    try {
      const result = await apiClient.comparePlans(selectedPlan1, selectedPlan2);
      setCompareResult(result);
      toast.success('Plans compared successfully');
    } catch (err) {
      console.error('Failed to compare plans:', err);
      toast.error('Failed to compare plans');
    }
  };

  const handleRollback = async () => {
    try {
      await apiClient.rollback();
      toast.success('Rollback initiated');
      setShowRollbackModal(false);
      fetchPlans();
    } catch (err) {
      console.error('Failed to rollback:', err);
      toast.error('Failed to rollback');
    }
  };

  if (loading) {
    return <div className="text-center p-8">Loading plans...</div>;
  }

  return (
    <div className="space-y-6">
      <div className="flex justify-between items-start">
        <div>
          <h1 className="section-title">Execution Plans</h1>
          <p className="section-description">
            Manage compiled plans and kernel configurations
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" onClick={() => setShowCompareModal(true)}>
            <GitCompare className="icon-standard mr-2" />
            Compare Plans
          </Button>
          <Button variant="destructive" onClick={() => setShowRollbackModal(true)}>
            <Undo2 className="icon-standard mr-2" />
            Rollback
          </Button>
          <Button>
            <Plus className="icon-standard mr-2" />
            Build Plan
          </Button>
        </div>
      </div>

      <Card className="card-standard">
        <CardHeader>
          <CardTitle>Plans</CardTitle>
        </CardHeader>
        <CardContent>
          <Table className="table-standard">
            <TableHeader>
              <TableRow>
                <TableHead>Plan ID</TableHead>
                <TableHead>CPID</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Created</TableHead>
                <TableHead>Metallib Hash</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {plans.map((plan) => (
                <TableRow key={plan.id}>
                  <TableCell className="table-cell-standard font-medium">{plan.id}</TableCell>
                  <TableCell className="table-cell-standard">{plan.cpid}</TableCell>
                  <TableCell className="table-cell-standard">
                    <Badge variant={plan.status === 'ready' ? 'default' : 'secondary'}>
                      {plan.status}
                    </Badge>
                  </TableCell>
                  <TableCell className="table-cell-standard">{new Date(plan.created_at).toLocaleString()}</TableCell>
                  <TableCell className="table-cell-standard font-mono text-xs">
                    {plan.metallib_hash?.substring(0, 16) || 'N/A'}
                  </TableCell>
                  <TableCell className="table-cell-standard">
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button variant="ghost" size="sm">
                          <MoreHorizontal className="h-4 w-4" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem onClick={() => handleRebuild(plan.id)}>
                          <RefreshCw className="mr-2 h-4 w-4" />
                          Rebuild
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => handleExportManifest(plan.id)}>
                          <Download className="mr-2 h-4 w-4" />
                          Export Manifest
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </TableCell>
                </TableRow>
              ))}
              {plans.length === 0 && (
                <TableRow>
                  <TableCell colSpan={6} className="table-cell-standard text-center text-muted-foreground">
                    No plans available
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {/* Compare Plans Modal */}
      <Dialog open={showCompareModal} onOpenChange={setShowCompareModal}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Compare Plans</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div>
              <Label>Plan 1</Label>
              <Select value={selectedPlan1} onValueChange={setSelectedPlan1}>
                <SelectTrigger>
                  <SelectValue placeholder="Select first plan" />
                </SelectTrigger>
                <SelectContent>
                  {plans.map((plan) => (
                    <SelectItem key={plan.id} value={plan.id}>
                      {plan.id} ({plan.cpid})
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div>
              <Label>Plan 2</Label>
              <Select value={selectedPlan2} onValueChange={setSelectedPlan2}>
                <SelectTrigger>
                  <SelectValue placeholder="Select second plan" />
                </SelectTrigger>
                <SelectContent>
                  {plans.map((plan) => (
                    <SelectItem key={plan.id} value={plan.id}>
                      {plan.id} ({plan.cpid})
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <Button onClick={handleComparePlans} disabled={!selectedPlan1 || !selectedPlan2}>
              Compare
            </Button>
            {compareResult && (
              <Alert>
                <AlertDescription>
                  <div className="space-y-2">
                    <p><strong>Comparison Results:</strong></p>
                    <p>Plan 1: {compareResult.plan_1}</p>
                    <p>Plan 2: {compareResult.plan_2}</p>
                    <p>Metallib Hash Changed: {compareResult.metallib_hash_changed ? 'Yes' : 'No'}</p>
                    {compareResult.differences.length > 0 && (
                      <div>
                        <p><strong>Differences:</strong></p>
                        <ul className="list-disc list-inside">
                          {compareResult.differences.map((diff, idx) => (
                            <li key={idx}>{diff}</li>
                          ))}
                        </ul>
                      </div>
                    )}
                    {compareResult.adapter_changes.added.length > 0 && (
                      <div>
                        <p><strong>Adapters Added:</strong></p>
                        <ul className="list-disc list-inside">
                          {compareResult.adapter_changes.added.map((adapter, idx) => (
                            <li key={idx}>{adapter}</li>
                          ))}
                        </ul>
                      </div>
                    )}
                    {compareResult.adapter_changes.removed.length > 0 && (
                      <div>
                        <p><strong>Adapters Removed:</strong></p>
                        <ul className="list-disc list-inside">
                          {compareResult.adapter_changes.removed.map((adapter, idx) => (
                            <li key={idx}>{adapter}</li>
                          ))}
                        </ul>
                      </div>
                    )}
                  </div>
                </AlertDescription>
              </Alert>
            )}
          </div>
        </DialogContent>
      </Dialog>

      {/* Rollback Modal */}
      <Dialog open={showRollbackModal} onOpenChange={setShowRollbackModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Confirm Rollback</DialogTitle>
          </DialogHeader>
          <Alert variant="destructive">
            <AlertDescription>
              This will rollback to the previous checkpoint. This action cannot be undone.
              Are you sure you want to proceed?
            </AlertDescription>
          </Alert>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowRollbackModal(false)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleRollback}>
              Confirm Rollback
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}