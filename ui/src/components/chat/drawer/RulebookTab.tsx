import React, { useMemo } from 'react';
import { FileText, Download } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { cn } from '@/lib/utils';
import { toast } from 'sonner';
import type { EvidenceItem } from '@/components/chat/ChatMessage';

interface RulebookTabProps {
  evidence: EvidenceItem[] | null;
  onViewDocument?: (documentId: string, pageNumber?: number, highlightText?: string) => void;
}

interface GroupedEvidence {
  documentId: string;
  documentName: string;
  items: EvidenceItem[];
}

function getRelevanceColor(score: number): string {
  if (score >= 0.8) return 'text-green-600';
  if (score >= 0.6) return 'text-yellow-600';
  return 'text-red-600';
}

function getRelevanceLabel(score: number): string {
  if (score >= 0.8) return 'High';
  if (score >= 0.6) return 'Medium';
  return 'Low';
}

export function RulebookTab({ evidence, onViewDocument }: RulebookTabProps) {
  // Group evidence by document
  const groupedEvidence = useMemo(() => {
    if (!evidence || evidence.length === 0) return [];

    const groups = new Map<string, GroupedEvidence>();

    evidence.forEach((item) => {
      const key = item.documentId;
      if (!groups.has(key)) {
        groups.set(key, {
          documentId: item.documentId,
          documentName: item.documentName,
          items: [],
        });
      }
      groups.get(key)!.items.push(item);
    });

    // Sort items within each group by relevance score (descending)
    groups.forEach((group) => {
      group.items.sort((a, b) => b.relevanceScore - a.relevanceScore);
    });

    return Array.from(groups.values()).sort((a, b) =>
      a.documentName.localeCompare(b.documentName)
    );
  }, [evidence]);

  const handleExportJSON = () => {
    if (!evidence || evidence.length === 0) {
      toast.error('No evidence to export');
      return;
    }

    try {
      const json = JSON.stringify(evidence, null, 2);
      const blob = new Blob([json], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `evidence-${Date.now()}.json`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      // Note: a.click() is fire-and-forget; we can only confirm download was initiated
      toast.success('JSON download started');
    } catch (error) {
      toast.error('Failed to initiate download');
      console.error('Export error:', error);
    }
  };

  const handleExportText = () => {
    if (!evidence || evidence.length === 0) {
      toast.error('No evidence to export');
      return;
    }

    try {
      const lines: string[] = [
        'Evidence Citations',
        '==================',
        '',
      ];

      groupedEvidence.forEach((group) => {
        lines.push(`Document: ${group.documentName}`);
        lines.push('---');
        group.items.forEach((item, idx) => {
          lines.push(`${idx + 1}. Page ${item.pageNumber ?? 'N/A'} | Relevance: ${(item.relevanceScore * 100).toFixed(1)}%`);
          lines.push(`   "${item.textPreview}"`);
          lines.push('');
        });
        lines.push('');
      });

      const text = lines.join('\n');
      const blob = new Blob([text], { type: 'text/plain' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `evidence-${Date.now()}.txt`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      // Note: a.click() is fire-and-forget; we can only confirm download was initiated
      toast.success('Text download started');
    } catch (error) {
      toast.error('Failed to initiate download');
      console.error('Export error:', error);
    }
  };

  if (!evidence || evidence.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-center">
        <FileText className="h-12 w-12 text-muted-foreground/50 mb-4" />
        <p className="text-sm text-muted-foreground">No citations available</p>
        <p className="text-xs text-muted-foreground mt-1">
          Evidence will appear here when documents are referenced
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between mb-4 pb-4 border-b">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-semibold">Citations</h3>
          <Badge variant="secondary" className="text-xs">
            {evidence.length}
          </Badge>
        </div>
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={handleExportJSON}
            className="gap-2"
          >
            <Download className="h-4 w-4" />
            JSON
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={handleExportText}
            className="gap-2"
          >
            <Download className="h-4 w-4" />
            Text
          </Button>
        </div>
      </div>

      {/* Citations list */}
      <ScrollArea className="flex-1">
        <div className="space-y-6 pr-4">
          {groupedEvidence.map((group) => (
            <div key={group.documentId} className="space-y-3">
              <div className="flex items-center gap-2">
                <FileText className="h-4 w-4 text-muted-foreground" />
                <span className="text-sm font-medium text-foreground">
                  {group.documentName}
                </span>
                <Badge variant="outline" className="text-xs">
                  {group.items.length} {group.items.length === 1 ? 'citation' : 'citations'}
                </Badge>
              </div>

              <div className="space-y-2 ml-6">
                {group.items.map((item) => {
                  const relevanceColor = getRelevanceColor(item.relevanceScore);
                  const relevanceLabel = getRelevanceLabel(item.relevanceScore);

                  return (
                    <div
                      key={item.chunkId}
                      className={cn(
                        'p-3 rounded-lg border bg-card transition-colors',
                        onViewDocument && 'cursor-pointer hover:bg-accent/50'
                      )}
                      onClick={() => {
                        if (onViewDocument) {
                          onViewDocument(
                            item.documentId,
                            item.pageNumber ?? undefined,
                            item.textPreview
                          );
                        }
                      }}
                    >
                      <div className="flex items-start justify-between gap-2 mb-2">
                        <div className="flex items-center gap-2 flex-wrap">
                          {item.pageNumber !== null && (
                            <Badge variant="secondary" className="text-xs">
                              p. {item.pageNumber}
                            </Badge>
                          )}
                          <Badge variant="outline" className={cn('text-xs', relevanceColor)}>
                            {relevanceLabel}
                          </Badge>
                          <span className="text-xs text-muted-foreground">
                            {(item.relevanceScore * 100).toFixed(1)}%
                          </span>
                        </div>
                      </div>
                      <p className="text-sm text-foreground/90 line-clamp-3">
                        "{item.textPreview}"
                      </p>
                    </div>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      </ScrollArea>
    </div>
  );
}
