import type { ApiClient } from '@/api/client';
import * as documentTypes from '@/api/document-types';
import { handleBlobResponse, getFilenameFromResponse } from '@/api/helpers';

/**
 * DocumentsService
 *
 * Handles all document-related API operations including:
 * - Document uploads, processing, and management
 * - Collection creation and management
 * - Evidence tracking and retrieval
 * - Document chunking and indexing
 * - RAG integration
 *
 * Citation: Extracted from client.ts lines 5853-6242
 */
export class DocumentsService {
  constructor(private client: ApiClient) {}

  // ============================================================================
  // Document API Methods
  // ============================================================================

  /**
   * Upload a document file
   *
   * POST /v1/documents (multipart/form-data)
   *
   * @param file - File to upload
   * @param name - Optional document name (defaults to filename)
   * @returns Uploaded document metadata
   */
  async uploadDocument(
    params: File | { file: File; name?: string; description?: string },
    name?: string
  ): Promise<documentTypes.Document> {
    const file = params instanceof File ? params : params.file;
    const providedName = params instanceof File ? name : params.name;
    const description = params instanceof File ? undefined : params.description;

    const formData = new FormData();
    formData.append('file', file);
    if (providedName) formData.append('name', providedName);
    if (description) formData.append('description', description);

    return this.client.request<documentTypes.Document>('/v1/documents/upload', {
      method: 'POST',
      body: formData,
      headers: {}, // Let browser set Content-Type for FormData
    });
  }

  /**
   * Process an uploaded document (parse, chunk, embed, index)
   *
   * POST /v1/documents/:id/process
   */
  async processDocument(
    documentId: string
  ): Promise<documentTypes.ProcessDocumentResponse> {
    return this.client.request<documentTypes.ProcessDocumentResponse>(
      `/v1/documents/${encodeURIComponent(documentId)}/process`,
      { method: 'POST' }
    );
  }

  /**
   * List all documents for the current tenant
   *
   * GET /v1/documents
   *
   * @returns Array of documents
   */
  async listDocuments(): Promise<documentTypes.Document[]> {
    return this.client.requestList<documentTypes.Document>('/v1/documents');
  }

  /**
   * Get a specific document by ID
   *
   * GET /v1/documents/:id
   *
   * @param documentId - Document ID
   * @returns Document metadata
   */
  async getDocument(documentId: string): Promise<documentTypes.Document> {
    return this.client.request<documentTypes.Document>(
      `/v1/documents/${encodeURIComponent(documentId)}`
    );
  }

  /**
   * Delete a document
   *
   * DELETE /v1/documents/:id
   *
   * @param documentId - Document ID
   */
  async deleteDocument(documentId: string): Promise<void> {
    await this.client.request<void>(
      `/v1/documents/${encodeURIComponent(documentId)}`,
      { method: 'DELETE' }
    );
  }

  /**
   * List chunks for a document
   *
   * GET /v1/documents/:id/chunks
   *
   * @param documentId - Document ID
   * @returns Array of document chunks
   */
  async listDocumentChunks(documentId: string): Promise<documentTypes.DocumentChunk[]> {
    return this.client.requestList<documentTypes.DocumentChunk>(
      `/v1/documents/${encodeURIComponent(documentId)}/chunks`
    );
  }

  /**
   * Download a document file
   *
   * GET /v1/documents/:id/download
   *
   * @param documentId - Document ID
   * @returns Blob of the document file
   */
  async downloadDocument(documentId: string): Promise<Blob> {
    const path = `/v1/documents/${encodeURIComponent(documentId)}/download`;
    const url = this.client.buildUrl(path);
    const token = this.client.getToken();
    const response = await fetch(url, {
      method: 'GET',
      headers: token ? { Authorization: `Bearer ${token}` } : undefined,
      credentials: 'omit',
    });

    return handleBlobResponse(response, { method: 'GET', path });
  }

  // ============================================================================
  // Collection API Methods
  // ============================================================================

  /**
   * Create a new collection
   *
   * POST /v1/collections
   *
   * @param name - Collection name
   * @param description - Optional description
   * @returns Created collection
   */
  async createCollection(
    name: string,
    description?: string
  ): Promise<documentTypes.Collection> {
    const request: documentTypes.CreateCollectionRequest = { name, description };
    return this.client.request<documentTypes.Collection>('/v1/collections', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  /**
   * List all collections for the current tenant
   *
   * GET /v1/collections
   *
   * @returns Array of collections
   */
  async listCollections(): Promise<documentTypes.Collection[]> {
    return this.client.requestList<documentTypes.Collection>('/v1/collections');
  }

  /**
   * Get a specific collection with documents
   *
   * GET /v1/collections/:id
   *
   * @param collectionId - Collection ID
   * @returns Collection detail with documents
   */
  async getCollection(collectionId: string): Promise<documentTypes.CollectionDetail> {
    return this.client.request<documentTypes.CollectionDetail>(
      `/v1/collections/${encodeURIComponent(collectionId)}`
    );
  }

  /**
   * List documents not yet in the specified collection
   *
   * GET /v1/collections/:id/available-documents
   *
   * @param collectionId - Collection ID
   * @returns Array of available documents
   */
  async listAvailableDocuments(collectionId: string): Promise<documentTypes.Document[]> {
    return this.client.requestList<documentTypes.Document>(
      `/v1/collections/${encodeURIComponent(collectionId)}/available-documents`
    );
  }

  /**
   * Delete a collection
   *
   * DELETE /v1/collections/:id
   *
   * @param collectionId - Collection ID
   */
  async deleteCollection(collectionId: string): Promise<void> {
    await this.client.request<void>(
      `/v1/collections/${encodeURIComponent(collectionId)}`,
      { method: 'DELETE' }
    );
  }

  /**
   * Add a document to a collection
   *
   * POST /v1/collections/:id/documents
   *
   * @param collectionId - Collection ID
   * @param documentId - Document ID to add
   */
  async addDocumentToCollection(
    collectionId: string,
    documentId: string
  ): Promise<void> {
    const request: documentTypes.AddDocumentRequest = { document_id: documentId };
    await this.client.request<void>(
      `/v1/collections/${encodeURIComponent(collectionId)}/documents`,
      {
        method: 'POST',
        body: JSON.stringify(request),
      }
    );
  }

  /**
   * Add multiple documents to a collection
   *
   * POST /v1/collections/:id/documents (bulk)
   */
  async addDocumentsToCollection(
    collectionId: string,
    documentIds: string[]
  ): Promise<void> {
    await this.client.request<void>(
      `/v1/collections/${encodeURIComponent(collectionId)}/documents`,
      {
        method: 'POST',
        body: JSON.stringify({ document_ids: documentIds }),
      }
    );
  }

  /**
   * Remove a document from a collection
   *
   * DELETE /v1/collections/:id/documents/:doc_id
   *
   * @param collectionId - Collection ID
   * @param documentId - Document ID to remove
   */
  async removeDocumentFromCollection(
    collectionId: string,
    documentId: string
  ): Promise<void> {
    await this.client.request<void>(
      `/v1/collections/${encodeURIComponent(collectionId)}/documents/${encodeURIComponent(documentId)}`,
      { method: 'DELETE' }
    );
  }

  // ============================================================================
  // Evidence API Methods
  // ============================================================================

  /**
   * List evidence entries with optional filters
   *
   * GET /v1/evidence
   *
   * @param query - Optional filter parameters
   * @returns Array of evidence entries
   */
  async listEvidence(query?: documentTypes.ListEvidenceQuery): Promise<documentTypes.Evidence[]> {
    const params = new URLSearchParams();
    if (query?.dataset_id) params.append('dataset_id', query.dataset_id);
    if (query?.adapter_id) params.append('adapter_id', query.adapter_id);
    if (query?.evidence_type) params.append('evidence_type', query.evidence_type);
    if (query?.confidence) params.append('confidence', query.confidence);
    if (query?.limit) params.append('limit', query.limit.toString());

    const queryString = params.toString();
    return this.client.requestList<documentTypes.Evidence>(
      `/v1/evidence${queryString ? `?${queryString}` : ''}`
    );
  }

  /**
   * Create a new evidence entry
   *
   * POST /v1/evidence
   *
   * @param request - Evidence creation request
   * @returns Created evidence entry
   */
  async createEvidence(
    request: documentTypes.CreateEvidenceRequest
  ): Promise<documentTypes.Evidence> {
    return this.client.request<documentTypes.Evidence>('/v1/evidence', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  /**
   * Get a specific evidence entry
   *
   * GET /v1/evidence/:id
   *
   * @param evidenceId - Evidence entry ID
   * @returns Evidence entry
   */
  async getEvidence(evidenceId: string): Promise<documentTypes.Evidence> {
    return this.client.request<documentTypes.Evidence>(
      `/v1/evidence/${encodeURIComponent(evidenceId)}`
    );
  }

  /**
   * Delete an evidence entry
   *
   * DELETE /v1/evidence/:id
   *
   * @param evidenceId - Evidence entry ID
   */
  async deleteEvidence(evidenceId: string): Promise<void> {
    await this.client.request<void>(
      `/v1/evidence/${encodeURIComponent(evidenceId)}`,
      { method: 'DELETE' }
    );
  }

  /**
   * Get evidence entries for a specific dataset
   *
   * GET /v1/datasets/:dataset_id/evidence
   *
   * @param datasetId - Dataset ID
   * @returns Array of evidence entries
   */
  async getDatasetEvidence(datasetId: string): Promise<documentTypes.Evidence[]> {
    return this.client.requestList<documentTypes.Evidence>(
      `/v1/datasets/${encodeURIComponent(datasetId)}/evidence`
    );
  }

  /**
   * Get evidence entries for a specific adapter
   *
   * GET /v1/adapters/:adapter_id/evidence
   *
   * @param adapterId - Adapter ID
   * @returns Array of evidence entries
   */
  async getAdapterEvidence(adapterId: string): Promise<documentTypes.Evidence[]> {
    return this.client.requestList<documentTypes.Evidence>(
      `/v1/adapters/${encodeURIComponent(adapterId)}/evidence`
    );
  }

  /**
   * Download an evidence bundle
   *
   * GET /v1/evidence/:id/download
   *
   * @param evidenceId - Evidence entry ID
   * @param options - Optional download options
   * @returns Blob and filename, triggers download by default
   */
  async downloadEvidence(
    evidenceId: string,
    options?: { filename?: string; triggerDownload?: boolean }
  ): Promise<{ blob: Blob; filename: string }> {
    const path = `/v1/evidence/${encodeURIComponent(evidenceId)}/download`;
    const url = this.client.buildUrl(path);
    const token = this.client.getToken();

    const response = await fetch(url, {
      method: 'GET',
      headers: token ? { Authorization: `Bearer ${token}` } : undefined,
      credentials: 'omit',
    });

    const blob = await handleBlobResponse(response, { method: 'GET', path });
    const filename = getFilenameFromResponse(response, options?.filename || `${evidenceId}.bin`);

    if (options?.triggerDownload !== false) {
      const blobUrl = window.URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = blobUrl;
      link.download = filename;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      window.URL.revokeObjectURL(blobUrl);
    }

    return { blob, filename };
  }
}
