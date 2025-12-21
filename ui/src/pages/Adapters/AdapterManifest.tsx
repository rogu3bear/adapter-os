// AdapterManifest - Manifest tab displaying adapter manifest JSON with download
// Shows manifest details in a formatted JSON viewer with copy and download functionality

import React, { useState } from 'react';
import { FileCode, Download, Copy, Check, ChevronDown, ChevronRight } from 'lucide-react';

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { EmptyState } from '@/components/ui/empty-state';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { Badge } from '@/components/ui/badge';
import { AdapterManifest as AdapterManifestType } from '@/api/adapter-types';
import { toast } from 'sonner';

interface AdapterManifestProps {
  adapterId: string;
  manifest: AdapterManifestType | null;
  isLoading: boolean;
}

export default function AdapterManifest({ adapterId, manifest, isLoading }: AdapterManifestProps) {
  const [copied, setCopied] = useState(false);

  if (isLoading && !manifest) {
    return <ManifestSkeleton />;
  }

  if (!manifest) {
    return (
      <EmptyState
        icon={FileCode}
        title="No manifest data"
        description="Manifest information is not available for this adapter."
      />
    );
  }

  // Handle copy to clipboard
  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(JSON.stringify(manifest, null, 2));
      setCopied(true);
      toast.success('Manifest copied to clipboard');
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      toast.error('Failed to copy manifest');
    }
  };

  // Handle download
  const handleDownload = () => {
    try {
      const blob = new Blob([JSON.stringify(manifest, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${adapterId}-manifest.json`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      toast.success('Manifest downloaded');
    } catch (err) {
      toast.error('Failed to download manifest');
    }
  };

  return (
    <div className="space-y-6">
      {/* Actions */}
      <div className="flex justify-end gap-2">
        <Button variant="outline" size="sm" onClick={handleCopy}>
          {copied ? (
            <>
              <Check className="h-4 w-4 mr-2" />
              Copied
            </>
          ) : (
            <>
              <Copy className="h-4 w-4 mr-2" />
              Copy
            </>
          )}
        </Button>
        <Button variant="outline" size="sm" onClick={handleDownload}>
          <Download className="h-4 w-4 mr-2" />
          Download
        </Button>
      </div>

      {/* Manifest Summary */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <FileCode className="h-5 w-5" />
            Manifest Summary
            <GlossaryTooltip brief="Key information extracted from the adapter manifest" />
          </CardTitle>
          <CardDescription>Core configuration and metadata</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            <ManifestField label="Version" value={manifest.version} />
            <ManifestField label="Name" value={manifest.name} />
            <ManifestField label="Base Model" value={manifest.base_model} />
            <ManifestField label="Rank" value={manifest.rank} />
            <ManifestField label="Alpha" value={manifest.alpha} />
            <ManifestField label="Hash" value={manifest.hash} truncate />
            {manifest.quantization && (
              <ManifestField
                label="Quantization"
                value={<Badge variant="outline">{manifest.quantization}</Badge>}
              />
            )}
            {manifest.dtype && (
              <ManifestField
                label="Data Type"
                value={<Badge variant="outline">{manifest.dtype}</Badge>}
              />
            )}
            <ManifestField
              label="Target Modules"
              value={manifest.target_modules?.length ?? 0}
              description={`${manifest.target_modules?.length ?? 0} modules`}
            />
          </div>
        </CardContent>
      </Card>

      {/* Target Modules */}
      {manifest.target_modules && manifest.target_modules.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <FileCode className="h-5 w-5" />
              Target Modules
              <GlossaryTooltip brief="Model layers/modules where LoRA adapters are applied" />
            </CardTitle>
            <CardDescription>
              {manifest.target_modules.length} modules configured for adaptation
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="flex flex-wrap gap-2">
              {manifest.target_modules.map((module, idx) => (
                <Badge key={idx} variant="secondary">
                  {module}
                </Badge>
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Description */}
      {manifest.description && (
        <Card>
          <CardHeader>
            <CardTitle>Description</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground">{manifest.description}</p>
          </CardContent>
        </Card>
      )}

      {/* Full Manifest JSON */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <FileCode className="h-5 w-5" />
            Full Manifest
            <GlossaryTooltip brief="Complete manifest in JSON format" />
          </CardTitle>
          <CardDescription>Raw manifest data</CardDescription>
        </CardHeader>
        <CardContent>
          <JsonViewer data={manifest} />
        </CardContent>
      </Card>
    </div>
  );
}

// Manifest field component
interface ManifestFieldProps {
  label: string;
  value: React.ReactNode;
  description?: string;
  truncate?: boolean;
}

function ManifestField({ label, value, description, truncate }: ManifestFieldProps) {
  return (
    <div className="space-y-1">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={`text-sm font-medium ${truncate ? 'truncate max-w-[200px]' : ''}`}>
        {value ?? 'N/A'}
      </div>
      {description && (
        <div className="text-xs text-muted-foreground">{description}</div>
      )}
    </div>
  );
}

// JSON viewer component with collapsible sections
interface JsonViewerProps {
  data: unknown;
}

function JsonViewer({ data }: JsonViewerProps) {
  return (
    <div className="bg-muted rounded-md p-4 overflow-auto max-h-[600px]">
      <pre className="text-xs font-mono">
        <JsonNode data={data} level={0} />
      </pre>
    </div>
  );
}

// Recursive JSON node component
interface JsonNodeProps {
  data: unknown;
  level: number;
  parentKey?: string;
}

function JsonNode({ data, level, parentKey }: JsonNodeProps) {
  const [isExpanded, setIsExpanded] = useState(level < 2); // Auto-expand first 2 levels
  const indent = '  '.repeat(level);

  if (data === null) {
    return <span className="text-muted-foreground">null</span>;
  }

  if (typeof data === 'string') {
    return <span className="text-green-600">"{data}"</span>;
  }

  if (typeof data === 'number') {
    return <span className="text-blue-600">{data}</span>;
  }

  if (typeof data === 'boolean') {
    return <span className="text-purple-600">{data.toString()}</span>;
  }

  if (Array.isArray(data)) {
    if (data.length === 0) {
      return <span>[]</span>;
    }

    return (
      <>
        <button
          onClick={() => setIsExpanded(!isExpanded)}
          className="inline-flex items-center hover:bg-muted-foreground/10 rounded px-1"
        >
          {isExpanded ? (
            <ChevronDown className="h-3 w-3" />
          ) : (
            <ChevronRight className="h-3 w-3" />
          )}
        </button>
        <span>[</span>
        {isExpanded ? (
          <>
            <span className="text-muted-foreground ml-2">({data.length} items)</span>
            {'\n'}
            {data.map((item, idx) => (
              <React.Fragment key={idx}>
                {indent}  <JsonNode data={item} level={level + 1} />
                {idx < data.length - 1 && ','}
                {'\n'}
              </React.Fragment>
            ))}
            {indent}
          </>
        ) : (
          <span className="text-muted-foreground"> {data.length} items... </span>
        )}
        <span>]</span>
      </>
    );
  }

  if (typeof data === 'object') {
    const entries = Object.entries(data as Record<string, unknown>);

    if (entries.length === 0) {
      return <span>{'{}'}</span>;
    }

    return (
      <>
        <button
          onClick={() => setIsExpanded(!isExpanded)}
          className="inline-flex items-center hover:bg-muted-foreground/10 rounded px-1"
        >
          {isExpanded ? (
            <ChevronDown className="h-3 w-3" />
          ) : (
            <ChevronRight className="h-3 w-3" />
          )}
        </button>
        <span>{'{'}</span>
        {isExpanded ? (
          <>
            <span className="text-muted-foreground ml-2">({entries.length} fields)</span>
            {'\n'}
            {entries.map(([key, value], idx) => (
              <React.Fragment key={key}>
                {indent}  <span className="text-cyan-600">"{key}"</span>: <JsonNode data={value} level={level + 1} parentKey={key} />
                {idx < entries.length - 1 && ','}
                {'\n'}
              </React.Fragment>
            ))}
            {indent}
          </>
        ) : (
          <span className="text-muted-foreground"> {entries.length} fields... </span>
        )}
        <span>{'}'}</span>
      </>
    );
  }

  return <span>{String(data)}</span>;
}

// Skeleton for loading state
function ManifestSkeleton() {
  return (
    <div className="space-y-6">
      <div className="flex justify-end gap-2">
        <Skeleton className="h-9 w-20" />
        <Skeleton className="h-9 w-28" />
      </div>
      <Card>
        <CardHeader>
          <Skeleton className="h-6 w-48" />
          <Skeleton className="h-4 w-64" />
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {[...Array(9)].map((_, i) => (
              <div key={i} className="space-y-1">
                <Skeleton className="h-3 w-20" />
                <Skeleton className="h-4 w-32" />
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
          <Skeleton className="h-[400px] w-full" />
        </CardContent>
      </Card>
    </div>
  );
}
