/**
 * Terminology Constants
 *
 * Maps technical terms to user-friendly labels for progressive disclosure UX.
 * These terms help non-technical users understand complex concepts.
 */

/**
 * General UI terminology
 */
export const TERMS = {
  // Common actions
  cancel: 'Cancel',
  create: 'Create',
  delete: 'Delete',
  save: 'Save',
  edit: 'Edit',

  // Dataset terminology
  dataset: 'Dataset',
  datasets: 'Datasets',
  datasetName: 'Dataset Name',
  datasetDescription: 'Description',
  selectDataset: 'Select a dataset',
  uploadDataset: 'Upload Dataset',
  createDataset: 'Create Dataset',
  deleteDataset: 'Delete Dataset',
  noDatasets: 'No datasets available',
  noDatasetsDescription: 'Create a dataset to get started',
  datasetRequired: 'Dataset is required',
  datasetValidating: 'Validating dataset...',
  datasetStatus: 'Dataset Status',

  // Documents
  documents: 'Documents',
} as const;

/**
 * Proof and verification terminology
 */
export const PROOF_TERMS = {
  // Router concepts
  q15_gates: {
    technical: 'Q15 Quantized Gates',
    friendly: 'Selection Confidence',
    description: 'How confident the system is in choosing this adapter',
  },
  entropy: {
    technical: 'Shannon Entropy',
    friendly: 'Decision Certainty',
    description: 'How certain the system was about the routing decision',
  },
  hash_chain: {
    technical: 'BLAKE3 Hash Chain',
    friendly: 'Audit Trail',
    description: 'Cryptographic proof that the decision was not tampered with',
  },
  top_k: {
    technical: 'Top-K Selection',
    friendly: 'Best Matches',
    description: 'The most relevant adapters considered for your request',
  },

  // Adapter concepts
  activation_percentage: {
    technical: 'Activation Percentage',
    friendly: 'Usage Level',
    description: 'How frequently this adapter has been used recently',
  },
  lifecycle_state: {
    technical: 'Lifecycle State',
    friendly: 'Ready Status',
    description: 'Whether the adapter is loaded and ready to respond',
  },

  // Evidence concepts
  confidence_high: {
    technical: 'High Confidence',
    friendly: 'Very Reliable',
    description: 'This source has been thoroughly verified',
  },
  confidence_medium: {
    technical: 'Medium Confidence',
    friendly: 'Mostly Reliable',
    description: 'This source has been partially verified',
  },
  confidence_low: {
    technical: 'Low Confidence',
    friendly: 'Needs Review',
    description: 'This source should be verified before relying on it',
  },

  // Document concepts
  chunk_count: {
    technical: 'Chunk Count',
    friendly: 'Searchable Sections',
    description: 'Number of indexed sections that can be searched',
  },
  embedding: {
    technical: 'Vector Embedding',
    friendly: 'Semantic Index',
    description: 'How the document is understood for semantic search',
  },
} as const;

/**
 * Evidence type labels
 */
export const EVIDENCE_TYPE_LABELS: Record<string, string> = {
  doc: 'Document',
  ticket: 'Ticket',
  commit: 'Code Commit',
  policy_approval: 'Policy Approval',
  data_agreement: 'Data Agreement',
  review: 'Review',
  audit: 'Audit Entry',
  other: 'Other',
};

/**
 * Document status labels
 */
export const DOCUMENT_STATUS_LABELS: Record<string, string> = {
  processing: 'Processing',
  indexed: 'Ready',
  failed: 'Failed',
};

/**
 * Lifecycle state labels (adapter memory states)
 */
export const LIFECYCLE_STATE_LABELS: Record<string, string> = {
  unloaded: 'Not Loaded',
  cold: 'Ready',
  warm: 'Standby',
  hot: 'Loaded',
  resident: 'Pinned',
};

/**
 * Router-specific term mappings
 */
const ROUTER_TERMS: Record<string, { friendly: string; description: string }> = {
  entropy: {
    friendly: 'Confidence',
    description: 'How confident the router is in its adapter selection',
  },
  tau: {
    friendly: 'Temperature',
    description: 'Controls randomness in adapter selection',
  },
  k_value: {
    friendly: 'Selection Count',
    description: 'Number of adapters selected for this request',
  },
  latency_ms: {
    friendly: 'Response Time',
    description: 'Time taken to make the routing decision',
  },
  gate_q15: {
    friendly: 'Gate Value',
    description: 'Quantized gate value (Q15 format)',
  },
  gate_float: {
    friendly: 'Gate Score',
    description: 'Normalized gate score (0-1)',
  },
} as const;

/**
 * Helper function to get user-friendly term
 * Supports both PROOF_TERMS and ROUTER_TERMS
 */
export function getFriendlyTerm(key: string): string {
  if (key in PROOF_TERMS) {
    return PROOF_TERMS[key as keyof typeof PROOF_TERMS]?.friendly ?? key;
  }
  if (key in ROUTER_TERMS) {
    return ROUTER_TERMS[key]?.friendly ?? key;
  }
  return key;
}

/**
 * Helper function to get term description
 * Supports both PROOF_TERMS and ROUTER_TERMS
 */
export function getTermDescription(key: string): string {
  if (key in PROOF_TERMS) {
    return PROOF_TERMS[key as keyof typeof PROOF_TERMS]?.description ?? '';
  }
  if (key in ROUTER_TERMS) {
    return ROUTER_TERMS[key]?.description ?? '';
  }
  return '';
}

/**
 * Format dataset source type for display
 */
export function formatSourceType(sourceType: string): string {
  const sourceTypeMap: Record<string, string> = {
    code_repo: 'Code Repository',
    uploaded_files: 'Uploaded Files',
    generated: 'Generated',
  };
  return sourceTypeMap[sourceType] || sourceType;
}

/**
 * Format dataset validation status for display
 */
export function formatValidationStatus(status: string): string {
  const statusMap: Record<string, string> = {
    draft: 'Draft',
    validating: 'Validating',
    valid: 'Valid',
    invalid: 'Invalid',
    failed: 'Failed',
  };
  return statusMap[status] || status;
}
