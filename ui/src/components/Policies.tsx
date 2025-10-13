import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from './ui/dropdown-menu';
import { Shield, Plus, CheckCircle, MoreHorizontal, FileSignature, GitCompare, Download } from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '../api/client';
import { Policy, User, SignPolicyResponse, PolicyComparisonResponse } from '../api/types';

interface PoliciesProps {
  user: User;
  selectedTenant: string;
}

export function Policies({ user, selectedTenant }: PoliciesProps) {
  const [policies, setPolicies] = useState<Policy[]>([]);
  const [loading, setLoading] = useState(true);
  const [showSignModal, setShowSignModal] = useState(false);
  const [showCompareModal, setShowCompareModal] = useState(false);
  const [selectedPolicy, setSelectedPolicy] = useState<Policy | null>(null);
  const [signResult, setSignResult] = useState<SignPolicyResponse | null>(null);
  const [compareResult, setCompareResult] = useState<PolicyComparisonResponse | null>(null);
  const [compareCpid2, setCompareCpid2] = useState('');

  useEffect(() => {
    const fetchPolicies = async () => {
      try {
        const data = await apiClient.listPolicies();
        setPolicies(data);
      } catch (err) {
        console.error('Failed to fetch policies:', err);
      } finally {
        setLoading(false);
      }
    };
    fetchPolicies();
  }, []);

  const handleSignPolicy = async (policy: Policy) => {
    try {
      const result = await apiClient.signPolicy(policy.cpid);
      setSignResult(result);
      setSelectedPolicy(policy);
      setShowSignModal(true);
      toast.success(`Policy ${policy.cpid} signed successfully`);
    } catch (err) {
      toast.error('Failed to sign policy');
      console.error(err);
    }
  };

  const handleComparePolicy = async () => {
    if (!selectedPolicy || !compareCpid2) {
      toast.error('Please select both policies to compare');
      return;
    }
    try {
      const result = await apiClient.comparePolicies(selectedPolicy.cpid, compareCpid2);
      setCompareResult(result);
      toast.success('Policy comparison complete');
    } catch (err) {
      toast.error('Failed to compare policies');
      console.error(err);
    }
  };

  const handleExportPolicy = async (policy: Policy) => {
    try {
      const result = await apiClient.exportPolicy(policy.cpid);
      const dataStr = JSON.stringify(result, null, 2);
      const dataBlob = new Blob([dataStr], { type: 'application/json' });
      const url = URL.createObjectURL(dataBlob);
      const link = document.createElement('a');
      link.href = url;
      link.download = `policy-${policy.cpid}.json`;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      URL.revokeObjectURL(url);
      toast.success(`Policy ${policy.cpid} exported`);
    } catch (err) {
      toast.error('Failed to export policy');
      console.error(err);
    }
  };

  if (loading) {
    return <div className="text-center p-8">Loading policies...</div>;
  }

  return (
    <div className="space-y-6">
      <div className="flex-between section-header">
        <div>
          <h1 className="section-title">Policy Management</h1>
          <p className="section-description">
            Review and apply security policies across control planes
          </p>
        </div>
        <Button>
          <Plus className="icon-standard mr-2" />
          New Policy
        </Button>
      </div>

      <Card className="card-standard">
        <CardHeader>
          <CardTitle>Active Policies</CardTitle>
        </CardHeader>
        <CardContent>
          <Table className="table-standard">
            <TableHeader>
              <TableRow>
                <TableHead>CPID</TableHead>
                <TableHead>Schema Hash</TableHead>
                <TableHead>Status</TableHead>
                <TableHead className="w-[100px]">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {policies.map((policy) => (
                <TableRow key={policy.cpid}>
                  <TableCell className="table-cell-standard font-medium">{policy.cpid}</TableCell>
                  <TableCell className="table-cell-standard font-mono text-xs">
                    {policy.schema_hash.substring(0, 16)}
                  </TableCell>
                  <TableCell className="table-cell-standard">
                    <Badge variant="default">
                      <CheckCircle className="icon-small mr-1" />
                      Active
                    </Badge>
                  </TableCell>
                  <TableCell className="table-cell-standard">
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button variant="ghost" size="sm">
                          <MoreHorizontal className="icon-standard" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem onClick={() => handleSignPolicy(policy)}>
                          <FileSignature className="icon-standard mr-2" />
                          Sign Policy
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => { setSelectedPolicy(policy); setShowCompareModal(true); }}>
                          <GitCompare className="icon-standard mr-2" />
                          Compare
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => handleExportPolicy(policy)}>
                          <Download className="icon-standard mr-2" />
                          Export
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </TableCell>
                </TableRow>
              ))}
              {policies.length === 0 && (
                <TableRow>
                  <TableCell colSpan={4} className="table-cell-standard text-center text-muted-foreground">
                    No policies configured
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {/* Sign Policy Modal */}
      <Dialog open={showSignModal} onOpenChange={setShowSignModal}>
        <DialogContent className="modal-standard">
          <DialogHeader>
            <DialogTitle>Policy Signature</DialogTitle>
          </DialogHeader>
          {signResult && (
            <div className="space-y-3">
              <div className="form-field">
                <p className="form-label">CPID</p>
                <p className="text-sm text-muted-foreground font-mono">{signResult.cpid}</p>
              </div>
              <div className="form-field">
                <p className="form-label">Signature</p>
                <p className="text-xs text-muted-foreground font-mono break-all">{signResult.signature}</p>
              </div>
              <div className="form-field">
                <p className="form-label">Signed By</p>
                <p className="text-sm text-muted-foreground">{signResult.signed_by}</p>
              </div>
              <div className="form-field">
                <p className="form-label">Signed At</p>
                <p className="text-sm text-muted-foreground">{new Date(signResult.signed_at).toLocaleString()}</p>
              </div>
            </div>
          )}
          <DialogFooter>
            <Button onClick={() => setShowSignModal(false)}>Close</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Compare Policies Modal */}
      <Dialog open={showCompareModal} onOpenChange={setShowCompareModal}>
        <DialogContent className="modal-large">
          <DialogHeader>
            <DialogTitle>Compare Policies</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div className="form-field">
              <label className="form-label">First Policy</label>
              <p className="text-sm text-muted-foreground font-mono">{selectedPolicy?.cpid}</p>
            </div>
            <div className="form-field">
              <label className="form-label">Second Policy CPID</label>
              <select 
                className="w-full p-2 border rounded"
                value={compareCpid2}
                onChange={(e) => setCompareCpid2(e.target.value)}
              >
                <option value="">Select policy...</option>
                {policies.filter(p => p.cpid !== selectedPolicy?.cpid).map((policy) => (
                  <option key={policy.cpid} value={policy.cpid}>{policy.cpid}</option>
                ))}
              </select>
            </div>
            {compareResult && (
              <div className="mt-4 space-y-3 border-t pt-4">
                <div className="form-field">
                  <p className="form-label">Differences ({compareResult.differences.length})</p>
                  <ul className="list-disc list-inside text-sm text-muted-foreground mt-2">
                    {compareResult.differences.map((diff, idx) => (
                      <li key={idx} className="font-mono text-xs">{diff}</li>
                    ))}
                  </ul>
                </div>
                {compareResult.added_keys.length > 0 && (
                  <div className="form-field">
                    <p className="form-label text-green-600">Added Keys</p>
                    <ul className="list-disc list-inside text-sm text-muted-foreground">
                      {compareResult.added_keys.map((key, idx) => (
                        <li key={idx} className="font-mono text-xs">{key}</li>
                      ))}
                    </ul>
                  </div>
                )}
                {compareResult.removed_keys.length > 0 && (
                  <div className="form-field">
                    <p className="form-label text-red-600">Removed Keys</p>
                    <ul className="list-disc list-inside text-sm text-muted-foreground">
                      {compareResult.removed_keys.map((key, idx) => (
                        <li key={idx} className="font-mono text-xs">{key}</li>
                      ))}
                    </ul>
                  </div>
                )}
              </div>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => { setShowCompareModal(false); setCompareResult(null); }}>
              Cancel
            </Button>
            <Button onClick={handleComparePolicy} disabled={!compareCpid2}>
              Compare
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}