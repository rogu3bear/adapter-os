// AdapterLineage - Lineage tab displaying adapter parent-child relationships
// Shows lineage tree with ancestors and descendants

import React from 'react';
import { GitBranch, GitCommit, GitMerge, ExternalLink, ArrowRight } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { EmptyState } from '@/components/ui/empty-state';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { AdapterLineageResponse } from '@/api/adapter-types';
import { formatDistanceToNow, parseISO } from 'date-fns';
import { formatRelativeTime } from '@/utils/format';

interface AdapterLineageProps {
  adapterId: string;
  lineage: AdapterLineageResponse | null;
  isLoading: boolean;
}

export default function AdapterLineage({ adapterId, lineage, isLoading }: AdapterLineageProps) {
  const navigate = useNavigate();

  if (isLoading && !lineage) {
    return <LineageSkeleton />;
  }

  if (!lineage) {
    return (
      <EmptyState
        icon={GitBranch}
        title="No lineage data"
        description="Lineage information will appear here if this adapter has parent or child relationships."
      />
    );
  }

  const hasAncestors = lineage.ancestors && lineage.ancestors.length > 0;
  const hasDescendants = lineage.descendants && lineage.descendants.length > 0;
  const hasHistory = lineage.history && lineage.history.length > 0;

  if (!hasAncestors && !hasDescendants && !hasHistory) {
    return (
      <EmptyState
        icon={GitBranch}
        title="No lineage relationships"
        description="This adapter has no parent or child adapters. It is a standalone adapter."
      />
    );
  }

  const handleNavigateToAdapter = (adapterIdToNavigate: string) => {
    navigate(`/adapters/${adapterIdToNavigate}`);
  };

  return (
    <div className="space-y-6">
      {/* Lineage Summary */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <GitBranch className="h-5 w-5" />
            Lineage Summary
            <GlossaryTooltip brief="Overview of adapter relationships and revision history" />
          </CardTitle>
          <CardDescription>Family tree and revision tracking</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <SummaryItem
              label="Ancestors"
              value={lineage.ancestors?.length ?? 0}
              description="Parent adapters in the lineage"
            />
            <SummaryItem
              label="Descendants"
              value={lineage.descendants?.length ?? 0}
              description="Child adapters derived from this one"
            />
            <SummaryItem
              label="Total Nodes"
              value={lineage.total_nodes ?? (hasAncestors || hasDescendants ? (lineage.ancestors?.length ?? 0) + (lineage.descendants?.length ?? 0) + 1 : 1)}
              description="Total adapters in the lineage tree"
            />
          </div>
        </CardContent>
      </Card>

      {/* Lineage Visualization */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <GitMerge className="h-5 w-5" />
            Lineage Tree
            <GlossaryTooltip brief="Visual representation of adapter lineage relationships" />
          </CardTitle>
          <CardDescription>Parent-child adapter relationships</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            {/* Ancestors */}
            {hasAncestors && (
              <div className="space-y-2">
                <h3 className="text-sm font-medium flex items-center gap-2">
                  <GitCommit className="h-4 w-4" />
                  Ancestors (Parents)
                </h3>
                <div className="pl-6 space-y-2">
                  {lineage.ancestors!.map((ancestor, idx) => (
                    <LineageNode
                      key={ancestor.adapter_id}
                      node={ancestor}
                      onNavigate={handleNavigateToAdapter}
                      isLast={idx === lineage.ancestors!.length - 1}
                    />
                  ))}
                </div>
              </div>
            )}

            {/* Current Adapter */}
            <div className="border-l-2 border-primary pl-6">
              <div className="flex items-center gap-3 p-3 bg-primary/10 rounded-md border border-primary/20">
                <GitCommit className="h-5 w-5 text-primary" />
                <div className="flex-1">
                  <div className="font-medium flex items-center gap-2">
                    {lineage.self_node?.adapter_name || adapterId}
                    <Badge variant="default">Current</Badge>
                  </div>
                  <div className="text-sm text-muted-foreground">
                    {lineage.self_node?.revision && `Revision: ${lineage.self_node.revision}`}
                    {lineage.self_node?.current_state && ` • State: ${lineage.self_node.current_state}`}
                  </div>
                </div>
              </div>
            </div>

            {/* Descendants */}
            {hasDescendants && (
              <div className="space-y-2">
                <h3 className="text-sm font-medium flex items-center gap-2">
                  <GitBranch className="h-4 w-4" />
                  Descendants (Children)
                </h3>
                <div className="pl-6 space-y-2">
                  {lineage.descendants!.map((descendant, idx) => (
                    <LineageNode
                      key={descendant.adapter_id}
                      node={descendant}
                      onNavigate={handleNavigateToAdapter}
                      isLast={idx === lineage.descendants!.length - 1}
                    />
                  ))}
                </div>
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Lineage History */}
      {hasHistory && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <GitCommit className="h-5 w-5" />
              Lineage History
              <GlossaryTooltip brief="Chronological record of lineage events" />
            </CardTitle>
            <CardDescription>Timeline of adapter derivations and changes</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-3">
              {lineage.history!.map((entry, idx) => (
                <HistoryEntry key={idx} entry={entry} isLast={idx === lineage.history!.length - 1} />
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Training Context */}
      {(lineage.lineage?.training_job_id || lineage.lineage?.dataset_id) && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <GitMerge className="h-5 w-5" />
              Training Context
              <GlossaryTooltip brief="Training job and dataset used to create this adapter" />
            </CardTitle>
            <CardDescription>Origin and training details</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {lineage.lineage.training_job_id && (
              <div className="flex items-center justify-between py-2 border-b">
                <span className="text-sm font-medium">Training Job</span>
                <div className="flex items-center gap-2">
                  <code className="text-sm bg-muted px-2 py-1 rounded">
                    {lineage.lineage.training_job_id}
                  </code>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => navigate(`/training/jobs/${lineage.lineage!.training_job_id}`)}
                  >
                    <ExternalLink className="h-4 w-4" />
                  </Button>
                </div>
              </div>
            )}
            {lineage.lineage.dataset_id && (
              <div className="flex items-center justify-between py-2">
                <span className="text-sm font-medium">Dataset</span>
                <div className="flex items-center gap-2">
                  <code className="text-sm bg-muted px-2 py-1 rounded">
                    {lineage.lineage.dataset_id}
                  </code>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => navigate(`/training/datasets/${lineage.lineage!.dataset_id}`)}
                  >
                    <ExternalLink className="h-4 w-4" />
                  </Button>
                </div>
              </div>
            )}
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// Summary item component
interface SummaryItemProps {
  label: string;
  value: number;
  description: string;
}

function SummaryItem({ label, value, description }: SummaryItemProps) {
  return (
    <div className="text-center p-4 border rounded-md">
      <div className="text-3xl font-bold text-primary">{value}</div>
      <div className="text-sm font-medium mt-1">{label}</div>
      <div className="text-xs text-muted-foreground mt-1">{description}</div>
    </div>
  );
}

// Lineage node component
interface LineageNodeProps {
  node: {
    adapter_id: string;
    adapter_name?: string;
    revision?: string;
    current_state?: string;
    fork_type?: string;
  };
  onNavigate: (adapterId: string) => void;
  isLast: boolean;
}

function LineageNode({ node, onNavigate, isLast }: LineageNodeProps) {
  return (
    <div className={`flex items-start gap-3 ${!isLast ? 'border-l-2 border-muted' : ''} pl-4 pb-3`}>
      <div className="mt-1.5">
        <GitCommit className="h-4 w-4 text-muted-foreground" />
      </div>
      <div className="flex-1">
        <div className="flex items-center gap-2">
          <button
            onClick={() => onNavigate(node.adapter_id)}
            className="text-sm font-medium hover:underline text-primary"
          >
            {node.adapter_name || node.adapter_id}
          </button>
          {node.fork_type && (
            <Badge variant="outline" className="text-xs">
              {node.fork_type}
            </Badge>
          )}
        </div>
        <div className="text-xs text-muted-foreground mt-0.5">
          {node.revision && `Rev: ${node.revision}`}
          {node.current_state && ` • ${node.current_state}`}
        </div>
      </div>
      <Button
        variant="ghost"
        size="sm"
        onClick={() => onNavigate(node.adapter_id)}
      >
        <ArrowRight className="h-4 w-4" />
      </Button>
    </div>
  );
}

// History entry component
interface HistoryEntryProps {
  entry: {
    timestamp: string;
    action: string;
    actor: string;
    details?: Record<string, unknown>;
  };
  isLast: boolean;
}

function HistoryEntry({ entry, isLast }: HistoryEntryProps) {
  const formatTime = (timestamp: string): string => {
    try {
      return formatRelativeTime(timestamp);
    } catch {
      return timestamp;
    }
  };

  return (
    <div className={`flex items-start gap-3 ${!isLast ? 'border-l-2 border-muted pl-4 pb-3' : 'pl-4'}`}>
      <div className="mt-1">
        <div className="h-2 w-2 rounded-full bg-primary" />
      </div>
      <div className="flex-1">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium">{entry.action}</span>
          <span className="text-xs text-muted-foreground">{formatTime(entry.timestamp)}</span>
        </div>
        <div className="text-xs text-muted-foreground mt-0.5">
          by {entry.actor}
        </div>
        {entry.details && Object.keys(entry.details).length > 0 && (
          <div className="mt-2 text-xs bg-muted p-2 rounded">
            <pre className="whitespace-pre-wrap">
              {JSON.stringify(entry.details, null, 2)}
            </pre>
          </div>
        )}
      </div>
    </div>
  );
}

// Skeleton for loading state
function LineageSkeleton() {
  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <Skeleton className="h-6 w-48" />
          <Skeleton className="h-4 w-64" />
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            {[...Array(3)].map((_, i) => (
              <div key={i} className="text-center p-4 border rounded-md">
                <Skeleton className="h-10 w-16 mx-auto" />
                <Skeleton className="h-4 w-24 mx-auto mt-2" />
                <Skeleton className="h-3 w-32 mx-auto mt-1" />
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <Skeleton className="h-6 w-48" />
          <Skeleton className="h-4 w-64" />
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            {[...Array(3)].map((_, i) => (
              <div key={i} className="flex items-center gap-3 pl-6">
                <Skeleton className="h-4 w-4" />
                <Skeleton className="h-16 flex-1" />
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
