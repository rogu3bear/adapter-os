import React from 'react';
import { ArrowRight, Filter, RefreshCw, Shield, ShieldAlert, ShieldCheck, Sparkles } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Separator } from '@/components/ui/separator';
import { Skeleton } from '@/components/ui/skeleton';
import type { LineageGraphResponse, LineageLevel, LineageNode, LineageNodeType } from '@/api/types';

type DirectionFilter = 'both' | 'upstream' | 'downstream';

export interface LineageViewerProps {
  title?: string;
  data: LineageGraphResponse | null;
  isLoading: boolean;
  onRefresh?: () => void;
  direction: DirectionFilter;
  includeEvidence: boolean;
  onChangeDirection: (dir: DirectionFilter) => void;
  onToggleEvidence: () => void;
  onNavigateNode: (node: LineageNode) => void;
  onLoadMore?: (level: LineageLevel, direction: 'upstream' | 'downstream') => void;
}

export function LineageViewer({
  title = 'Lineage',
  data,
  isLoading,
  onRefresh,
  direction,
  includeEvidence,
  onChangeDirection,
  onToggleEvidence,
  onNavigateNode,
  onLoadMore,
}: LineageViewerProps) {
  const showUpstream = direction === 'both' || direction === 'upstream';
  const showDownstream = direction === 'both' || direction === 'downstream';

  return (
    <Card>
      <CardHeader className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
        <CardTitle className="flex items-center gap-2">
          <Sparkles className="h-5 w-5 text-primary" />
          {title}
        </CardTitle>
        <div className="flex flex-wrap gap-2">
          <FilterPill
            label="Both"
            active={direction === 'both'}
            onClick={() => onChangeDirection('both')}
          />
          <FilterPill
            label="Upstream"
            active={direction === 'upstream'}
            onClick={() => onChangeDirection('upstream')}
          />
          <FilterPill
            label="Downstream"
            active={direction === 'downstream'}
            onClick={() => onChangeDirection('downstream')}
          />
          <FilterPill
            label="Evidence"
            active={includeEvidence}
            onClick={onToggleEvidence}
          />
          {onRefresh && (
            <Button variant="ghost" size="sm" onClick={onRefresh} className="gap-2">
              <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
              Refresh
            </Button>
          )}
        </div>
      </CardHeader>
      <CardContent className="space-y-6">
        {isLoading && !data && <LineageSkeleton />}
        {data && (
          <>
            <RootNode node={data.root} onNavigate={onNavigateNode} />
            <Separator />
            {showUpstream && (
              <LineageSection
                title="Upstream"
                levels={data.upstream}
                direction="upstream"
                onNavigate={onNavigateNode}
                onLoadMore={onLoadMore}
              />
            )}
            {showDownstream && (
              <LineageSection
                title="Downstream"
                levels={data.downstream}
                direction="downstream"
                onNavigate={onNavigateNode}
                onLoadMore={onLoadMore}
              />
            )}
            {includeEvidence && data.evidence && data.evidence.length > 0 && (
              <LineageSection
                title="Evidence"
                levels={[
                  {
                    type: 'evidence',
                    nodes: data.evidence,
                    label: 'Evidence entries',
                  },
                ]}
                direction="upstream"
                onNavigate={onNavigateNode}
              />
            )}
          </>
        )}
        {!isLoading && !data && (
          <div className="text-sm text-muted-foreground">No lineage available.</div>
        )}
      </CardContent>
    </Card>
  );
}

function LineageSection({
  title,
  levels,
  direction,
  onNavigate,
  onLoadMore,
}: {
  title: string;
  levels?: LineageLevel[];
  direction: 'upstream' | 'downstream';
  onNavigate: (node: LineageNode) => void;
  onLoadMore?: (level: LineageLevel, direction: 'upstream' | 'downstream') => void;
}) {
  if (!levels || levels.length === 0) return null;
  return (
    <div className="space-y-3">
      <div className="text-sm font-semibold text-muted-foreground">{title}</div>
      <div className="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
        {levels.map((level) => (
          <LineageColumn
            key={`${direction}-${level.type}`}
            level={level}
            onNavigate={onNavigate}
            onLoadMore={onLoadMore ? () => onLoadMore(level, direction) : undefined}
          />
        ))}
      </div>
    </div>
  );
}

function LineageColumn({
  level,
  onNavigate,
  onLoadMore,
}: {
  level: LineageLevel;
  onNavigate: (node: LineageNode) => void;
  onLoadMore?: () => void;
}) {
  return (
    <div className="space-y-2 rounded-lg border p-3">
      <div className="flex items-center justify-between gap-2">
        <div className="text-sm font-medium capitalize">
          {level.label || level.type.replace('_', ' ')}
        </div>
        {level.total !== undefined && (
          <span className="text-xs text-muted-foreground">
            {level.nodes.length}/{level.total}
          </span>
        )}
      </div>
      <div className="space-y-2">
        {level.nodes.map((node) => (
          <LineageNodeChip key={node.id} node={node} onClick={() => onNavigate(node)} />
        ))}
      </div>
      {(level.has_more || level.next_cursor) && onLoadMore && (
        <Button variant="outline" size="sm" className="w-full" onClick={onLoadMore}>
          See more
        </Button>
      )}
    </div>
  );
}

function LineageNodeChip({ node, onClick }: { node: LineageNode; onClick: () => void }) {
  const trustTone = node.trust_state === 'blocked' || node.trust_state === 'needs_approval' ? 'destructive' : 'outline';
  const trustLabel = node.trust_state ? `Trust: ${node.trust_state}` : undefined;
  const healthLabel = node.adapter_health ? `Health: ${node.adapter_health}` : undefined;
  const badges = node.badges ?? [];
  const hasBadges = badges.length > 0;

  return (
    <button
      onClick={onClick}
      className="w-full rounded-md border px-3 py-2 text-left transition hover:border-primary hover:bg-primary/5"
    >
      <div className="flex items-center justify-between gap-2">
        <div className="font-medium text-sm">{node.label}</div>
        <ArrowRight className="h-4 w-4 text-muted-foreground" />
      </div>
      {node.subtitle && <div className="text-xs text-muted-foreground">{node.subtitle}</div>}
      <div className="mt-1 flex flex-wrap gap-1">
        {trustLabel && <Badge variant={trustTone}>{trustLabel}</Badge>}
        {healthLabel && (
          <Badge variant={node.adapter_health === 'healthy' ? 'outline' : 'destructive'}>
            {healthLabel}
          </Badge>
        )}
        {hasBadges &&
          badges.map((b, idx) => (
            <Badge
              key={idx}
              variant={
                b.tone === 'danger'
                  ? 'destructive'
                  : b.tone === 'warning'
                  ? 'secondary'
                  : b.tone === 'success'
                  ? 'default'
                  : 'outline'
              }
            >
              {b.label}
            </Badge>
          ))}
      </div>
    </button>
  );
}

function RootNode({ node, onNavigate }: { node: LineageNode; onNavigate: (node: LineageNode) => void }) {
  const icon = resolveRootIcon(node.type);
  return (
    <div className="flex items-center justify-between rounded-lg border bg-muted/40 px-4 py-3">
      <div className="flex items-center gap-2">
        {icon}
        <div>
          <div className="text-sm font-semibold">{node.label}</div>
          {node.subtitle && <div className="text-xs text-muted-foreground">{node.subtitle}</div>}
        </div>
      </div>
      <Button variant="outline" size="sm" onClick={() => onNavigate(node)}>
        Open
      </Button>
    </div>
  );
}

function resolveRootIcon(type: LineageNodeType) {
  if (type === 'adapter_version') {
    return <ShieldCheck className="h-5 w-5 text-primary" />;
  }
  if (type === 'dataset_version' || type === 'dataset') {
    return <Shield className="h-5 w-5 text-primary" />;
  }
  if (type === 'training_job') {
    return <ShieldAlert className="h-5 w-5 text-primary" />;
  }
  return <Sparkles className="h-5 w-5 text-primary" />;
}

function FilterPill({ label, active, onClick }: { label: string; active: boolean; onClick: () => void }) {
  return (
    <Button
      size="sm"
      variant={active ? 'default' : 'outline'}
      onClick={onClick}
      className="gap-1"
    >
      <Filter className="h-4 w-4" />
      {label}
    </Button>
  );
}

function LineageSkeleton() {
  return (
    <div className="space-y-3">
      <Skeleton className="h-10 w-full" />
      <div className="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
        {[...Array(3)].map((_, idx) => (
          <div key={idx} className="space-y-2 rounded-lg border p-3">
            <Skeleton className="h-4 w-24" />
            {[...Array(3)].map((__, jdx) => (
              <Skeleton key={jdx} className="h-10 w-full" />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}
