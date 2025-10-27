import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Alert, AlertDescription } from './ui/alert';
import { toast } from 'sonner';
import apiClient from '../api/client';
import { User } from '../api/types';
import { logger, toError } from '../utils/logger';
import { ArrowUp, History, Download, Undo2, Play, CheckCircle, XCircle, AlertTriangle } from 'lucide-react';

interface PromotionProps {
  user: User;
  selectedTenant: string;
}

export function Promotion({ user, selectedTenant }: PromotionProps) {
  const [cpid, setCpid] = useState('');
  const [gates, setGates] = useState<any[]>([]);
  const [dryRunResult, setDryRunResult] = useState<any | null>(null);
  const [history, setHistory] = useState<any[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchHistory();
  }, []);

  const fetchHistory = async () => {
    try {
      const data = await apiClient.getPromotionHistory();
      setHistory(data);
    } catch (err) {
      toast.error('Failed to load history');
    }
  };

  const handleDryRun = async () => {
    setLoading(true);
    try {
      const result = await apiClient.dryRunPromotion(cpid);
      setDryRunResult(result);
    } catch (err) {
      setError('Dry run failed');
    } finally {
      setLoading(false);
    }
  };

  const handleCheckGates = async () => {
    setLoading(true);
    try {
      const data = await apiClient.getPromotionGates(cpid);
      setGates(data);
    } catch (err) {
      setError('Gate check failed');
    } finally {
      setLoading(false);
    }
  };

  const handlePromote = async () => {
    setLoading(true);
    try {
      await apiClient.promote({ cpid });
      toast.success('Promoted successfully');
      fetchHistory();
    } catch (err) {
      setError('Promotion failed');
    } finally {
      setLoading(false);
    }
  };

  const handleRollback = async () => {
    setLoading(true);
    try {
      await apiClient.rollback();
      toast.success('Rollback successful');
      fetchHistory();
    } catch (err) {
      setError('Rollback failed');
    } finally {
      setLoading(false);
    }
  };

  const allGatesPassed = gates.every(g => g.status === 'passed');

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle>Promotion Controls</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div>
            <Label>CPID</Label>
            <Input value={cpid} onChange={(e) => setCpid(e.target.value)} />
          </div>
          <div className="flex gap-2">
            <Button onClick={handleDryRun} disabled={loading}><Play className="mr-2" /> Dry Run</Button>
            <Button onClick={handleCheckGates} disabled={loading}><CheckCircle className="mr-2" /> Check Gates</Button>
            <Button onClick={handlePromote} disabled={loading || !allGatesPassed}><ArrowUp className="mr-2" /> Promote</Button>
            <Button variant="destructive" onClick={handleRollback} disabled={loading}><Undo2 className="mr-2" /> Rollback</Button>
          </div>
          {error && <Alert variant="destructive"><AlertDescription>{error}</AlertDescription></Alert>}
        </CardContent>
      </Card>

      {/* Gate Visualization */}
      {gates.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle>Gate Status</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-2">
              {gates.map((gate, idx) => (
                <div key={idx} className="flex items-center justify-between p-2 border rounded">
                  <span>{gate.name}</span>
                  <Badge variant={gate.status === 'passed' ? 'default' : 'destructive'}>{gate.status}</Badge>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Dry Run Preview */}
      {dryRunResult && (
        <Card>
          <CardHeader>
            <CardTitle>Dry Run Preview</CardTitle>
          </CardHeader>
          <CardContent>
            <pre className="text-sm overflow-auto max-h-48">{JSON.stringify(dryRunResult, null, 2)}</pre>
          </CardContent>
        </Card>
      )}

      {/* Promotion History */}
      <Card>
        <CardHeader>
          <CardTitle>Promotion History</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>CPID</TableHead>
                <TableHead>By</TableHead>
                <TableHead>Date</TableHead>
                <TableHead>Status</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {history.map((entry, idx) => (
                <TableRow key={idx}>
                  <TableCell>{entry.cpid}</TableCell>
                  <TableCell>{entry.promoted_by}</TableCell>
                  <TableCell>{new Date(entry.promoted_at).toLocaleString()}</TableCell>
                  <TableCell><Badge>{entry.status}</Badge></TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  );
}
