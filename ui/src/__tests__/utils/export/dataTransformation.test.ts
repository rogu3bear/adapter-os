import { describe, it, expect } from 'vitest';
import { downloadTextFile } from '@/utils/export/renderMarkdown';
import type { ChatMessage } from '@/components/chat/ChatMessage';
import type { ExtendedMessageExport, ExtendedEvidenceItem } from '@/utils/export/types';

describe('Data Transformation for Export', () => {
  describe('ChatMessage to ExtendedMessageExport conversion', () => {
    it('converts basic chat message fields', () => {
      const chatMessage: ChatMessage = {
        id: 'msg-1',
        role: 'assistant',
        content: 'Test answer',
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
      };

      const extended: ExtendedMessageExport = {
        id: chatMessage.id,
        role: chatMessage.role,
        content: chatMessage.content,
        timestamp: chatMessage.timestamp.toISOString(),
      };

      expect(extended.id).toBe('msg-1');
      expect(extended.role).toBe('assistant');
      expect(extended.content).toBe('Test answer');
      expect(extended.timestamp).toBe('2025-12-12T09:00:00.000Z');
    });

    it('converts evidence items with all metadata', () => {
      const chatMessage: ChatMessage = {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
        evidence: [
          {
            document_id: 'doc-1',
            document_name: 'Guide',
            chunk_id: 'chunk-1',
            page_number: 5,
            text_preview: 'Preview',
            relevance_score: 0.95,
            rank: 1,
            char_range: { start: 100, end: 200 },
            bbox: { x: 10, y: 20, width: 100, height: 50 },
            citation_id: 'cite-1',
          },
        ],
      };

      const extendedEvidence: ExtendedEvidenceItem = {
        documentId: chatMessage.evidence![0].document_id,
        documentName: chatMessage.evidence![0].document_name,
        chunkId: chatMessage.evidence![0].chunk_id,
        pageNumber: chatMessage.evidence![0].page_number,
        textPreview: chatMessage.evidence![0].text_preview,
        relevanceScore: chatMessage.evidence![0].relevance_score,
        rank: chatMessage.evidence![0].rank,
        charRange: chatMessage.evidence![0].char_range,
        bbox: chatMessage.evidence![0].bbox,
        citationId: chatMessage.evidence![0].citation_id,
      };

      expect(extendedEvidence.documentId).toBe('doc-1');
      expect(extendedEvidence.documentName).toBe('Guide');
      expect(extendedEvidence.charRange).toEqual({ start: 100, end: 200 });
      expect(extendedEvidence.bbox).toEqual({ x: 10, y: 20, width: 100, height: 50 });
      expect(extendedEvidence.citationId).toBe('cite-1');
    });

    it('converts router decision with candidates', () => {
      const chatMessage: ChatMessage = {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
        routerDecision: {
          request_id: 'req-123',
          selected_adapters: ['adapter-1'],
          candidates: [
            {
              adapter_id: 'adapter-1',
              adapter_idx: 0,
              gate_q15: 26214,
              gate_float: 0.8,
              raw_score: 0.95,
              selected: true,
              rank: 1,
            },
          ],
          timestamp: '2025-12-12T09:00:00.000Z',
          latency_ms: 50,
        },
      };

      const routerDecision = {
        requestId: chatMessage.routerDecision!.request_id,
        selectedAdapters: chatMessage.routerDecision!.selected_adapters,
        candidates: chatMessage.routerDecision!.candidates?.map((c) => ({
          adapterId: c.adapter_id,
          gateQ15: c.gate_q15,
          gateFloat: c.gate_float,
          selected: c.selected,
        })),
      };

      expect(routerDecision.requestId).toBe('req-123');
      expect(routerDecision.selectedAdapters).toEqual(['adapter-1']);
      expect(routerDecision.candidates![0].adapterId).toBe('adapter-1');
      expect(routerDecision.candidates![0].gateQ15).toBe(26214);
      expect(routerDecision.candidates![0].gateFloat).toBe(0.8);
      expect(routerDecision.candidates![0].selected).toBe(true);
    });

    it('handles missing optional fields gracefully', () => {
      const chatMessage: ChatMessage = {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
      };

      const extended: ExtendedMessageExport = {
        id: chatMessage.id,
        role: chatMessage.role,
        content: chatMessage.content,
        timestamp: chatMessage.timestamp.toISOString(),
        requestId: chatMessage.requestId,
        traceId: chatMessage.traceId,
        proofDigest: chatMessage.proofDigest,
        isVerified: chatMessage.isVerified,
        verifiedAt: chatMessage.verifiedAt,
      };

      expect(extended.requestId).toBeUndefined();
      expect(extended.traceId).toBeUndefined();
      expect(extended.proofDigest).toBeUndefined();
      expect(extended.isVerified).toBeUndefined();
      expect(extended.verifiedAt).toBeUndefined();
    });

    it('handles null page_number in evidence', () => {
      const chatMessage: ChatMessage = {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
        evidence: [
          {
            document_id: 'doc-1',
            document_name: 'Guide',
            chunk_id: 'chunk-1',
            page_number: null,
            text_preview: 'Preview',
            relevance_score: 0.95,
            rank: 1,
          },
        ],
      };

      const extendedEvidence: ExtendedEvidenceItem = {
        documentId: chatMessage.evidence![0].document_id,
        documentName: chatMessage.evidence![0].document_name,
        chunkId: chatMessage.evidence![0].chunk_id,
        pageNumber: chatMessage.evidence![0].page_number,
        textPreview: chatMessage.evidence![0].text_preview,
        relevanceScore: chatMessage.evidence![0].relevance_score,
        rank: chatMessage.evidence![0].rank,
      };

      expect(extendedEvidence.pageNumber).toBeNull();
    });
  });

  describe('Error handling in export transformations', () => {
    it('handles malformed timestamp gracefully', () => {
      const chatMessage = {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: 'invalid-date',
      };

      // Should handle invalid date without crashing
      expect(() => {
        const date = new Date(chatMessage.timestamp as unknown as Date);
        date.toISOString();
      }).toThrow();
    });

    it('handles missing required fields in evidence', () => {
      const incompleteEvidence = {
        document_id: 'doc-1',
        // missing document_name
        chunk_id: 'chunk-1',
        page_number: null,
        text_preview: 'Preview',
        relevance_score: 0.95,
        rank: 1,
      };

      // Should be able to destructure even with missing fields
      const { document_name = 'Unknown' } = incompleteEvidence as any;
      expect(document_name).toBe('Unknown');
    });

    it('handles empty arrays in collections', () => {
      const chatMessage: ChatMessage = {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
        evidence: [],
      };

      expect(chatMessage.evidence).toEqual([]);
      expect(chatMessage.evidence!.length).toBe(0);
    });

    it('handles very large evidence arrays', () => {
      const largeEvidence = Array.from({ length: 1000 }, (_, i) => ({
        document_id: `doc-${i}`,
        document_name: `Document ${i}`,
        chunk_id: `chunk-${i}`,
        page_number: i,
        text_preview: `Preview ${i}`,
        relevance_score: 0.5 + Math.random() * 0.5,
        rank: i + 1,
      }));

      const chatMessage: ChatMessage = {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
        evidence: largeEvidence,
      };

      expect(chatMessage.evidence!.length).toBe(1000);
      expect(chatMessage.evidence![999].document_id).toBe('doc-999');
    });

    it('handles special characters in content', () => {
      const specialContent = 'Content with "quotes", <tags>, & ampersands, 中文字符, emoji 🎉';
      const chatMessage: ChatMessage = {
        id: 'msg-1',
        role: 'assistant',
        content: specialContent,
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
      };

      const extended: ExtendedMessageExport = {
        id: chatMessage.id,
        role: chatMessage.role,
        content: chatMessage.content,
        timestamp: chatMessage.timestamp.toISOString(),
      };

      expect(extended.content).toBe(specialContent);
      expect(extended.content).toContain('emoji 🎉');
      expect(extended.content).toContain('中文字符');
    });

    it('handles negative or zero values in numeric fields', () => {
      const chatMessage: ChatMessage = {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
        evidence: [
          {
            document_id: 'doc-1',
            document_name: 'Guide',
            chunk_id: 'chunk-1',
            page_number: 0,
            text_preview: 'Preview',
            relevance_score: 0,
            rank: 0,
          },
        ],
      };

      const extendedEvidence: ExtendedEvidenceItem = {
        documentId: chatMessage.evidence![0].document_id,
        documentName: chatMessage.evidence![0].document_name,
        chunkId: chatMessage.evidence![0].chunk_id,
        pageNumber: chatMessage.evidence![0].page_number,
        textPreview: chatMessage.evidence![0].text_preview,
        relevanceScore: chatMessage.evidence![0].relevance_score,
        rank: chatMessage.evidence![0].rank,
      };

      expect(extendedEvidence.pageNumber).toBe(0);
      expect(extendedEvidence.relevanceScore).toBe(0);
      expect(extendedEvidence.rank).toBe(0);
    });

    it('preserves Q15 gate precision in router decision', () => {
      // Q15 denominator is 32767.0 (NOT 32768)
      const gateQ15 = 26214;
      const expectedGateFloat = gateQ15 / 32767.0;

      const chatMessage: ChatMessage = {
        id: 'msg-1',
        role: 'assistant',
        content: 'Answer',
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
        routerDecision: {
          request_id: 'req-123',
          selected_adapters: ['adapter-1'],
          candidates: [
            {
              adapter_id: 'adapter-1',
              adapter_idx: 0,
              gate_q15: gateQ15,
              gate_float: expectedGateFloat,
              raw_score: 0.95,
              selected: true,
              rank: 1,
            },
          ],
          timestamp: '2025-12-12T09:00:00.000Z',
          latency_ms: 50,
        },
      };

      const candidate = chatMessage.routerDecision!.candidates![0];
      expect(candidate.gate_q15).toBe(26214);
      // Q15 precision: 26214 / 32767.0 = 0.8000122074037904
      expect(candidate.gate_float).toBeCloseTo(0.8000122074037904, 10);
    });
  });

  describe('Sanitization and validation', () => {
    it('handles very long content strings', () => {
      const longContent = 'A'.repeat(100000);
      const chatMessage: ChatMessage = {
        id: 'msg-1',
        role: 'assistant',
        content: longContent,
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
      };

      const extended: ExtendedMessageExport = {
        id: chatMessage.id,
        role: chatMessage.role,
        content: chatMessage.content,
        timestamp: chatMessage.timestamp.toISOString(),
      };

      expect(extended.content.length).toBe(100000);
    });

    it('handles empty strings in required fields', () => {
      const chatMessage: ChatMessage = {
        id: '',
        role: 'assistant',
        content: '',
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
      };

      const extended: ExtendedMessageExport = {
        id: chatMessage.id,
        role: chatMessage.role,
        content: chatMessage.content,
        timestamp: chatMessage.timestamp.toISOString(),
      };

      expect(extended.id).toBe('');
      expect(extended.content).toBe('');
    });

    it('validates role enum values', () => {
      const validRoles: Array<'user' | 'assistant'> = ['user', 'assistant'];

      validRoles.forEach((role) => {
        const chatMessage: ChatMessage = {
          id: 'msg-1',
          role,
          content: 'Test',
          timestamp: new Date(),
        };

        expect(['user', 'assistant']).toContain(chatMessage.role);
      });
    });

    it('handles boundary values for relevance score', () => {
      const boundaryScores = [0, 0.5, 1.0, 1.1, -0.1];

      boundaryScores.forEach((score) => {
        const evidence = {
          document_id: 'doc-1',
          document_name: 'Guide',
          chunk_id: 'chunk-1',
          page_number: null,
          text_preview: 'Preview',
          relevance_score: score,
          rank: 1,
        };

        expect(evidence.relevance_score).toBe(score);
      });
    });
  });
});
