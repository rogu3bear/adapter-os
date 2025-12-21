export interface ExtendedExportMetadata {
  exportId: string;
  exportTimestamp: string;
  entityType: 'chat_session' | 'dataset' | 'adapter';
  entityId: string;
  entityName: string;
  determinismMode?: 'deterministic' | 'adaptive';
  determinismState?: 'verified' | 'unverified' | 'approximate';
  datasetVersionId?: string;
  adapterStack?: {
    stackId: string;
    stackName?: string;
    adapters: Array<{
      adapterId: string;
      version?: string;
      gate?: number;
    }>;
  };
  tenantId?: string;
  collectionId?: string;
}

export interface ExtendedMessageExport {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: string;
  requestId?: string;
  traceId?: string;
  proofDigest?: string;
  evidence?: ExtendedEvidenceItem[];
  routerDecision?: RouterDecisionExport;
  isVerified?: boolean;
  verifiedAt?: string;
  /** Adapter stack used for this specific turn */
  adapterStackSnapshot?: {
    stackId: string;
    stackName?: string;
    adapters: Array<{
      adapterId: string;
      version?: string;
      gate?: number;
    }>;
  };
  /** Dataset version ID used for this turn's RAG context */
  datasetVersionId?: string;
}

export interface ExtendedEvidenceItem {
  documentId: string;
  documentName: string;
  chunkId: string;
  pageNumber: number | null;
  textPreview: string;
  relevanceScore: number;
  rank: number;
  charRange?: { start: number; end: number };
  bbox?: { x: number; y: number; width: number; height: number };
  citationId?: string;
}

export interface RouterDecisionExport {
  requestId: string;
  selectedAdapters: string[];
  candidates?: Array<{
    adapterId: string;
    gateQ15: number;
    gateFloat: number;
    selected: boolean;
  }>;
  entropy?: number;
}

export interface EvidenceBundleExport {
  schemaVersion: string;
  exportTimestamp: string;
  exportId: string;
  traces: Array<{
    traceId: string;
    backendId: string;
  }>;
  evidence: ExtendedEvidenceItem[];
  signatures: Array<{
    traceId: string;
    signature: string;
    signedAt: string;
  }>;
  checksums: {
    bundleHash: string;
  };
  datasetVersionId?: string;
}

export type ExportFormat = 'markdown' | 'json' | 'pdf' | 'evidence-bundle';
export type ExportScope = 'answer' | 'session' | 'full';
