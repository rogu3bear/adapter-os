import { describe, it, expectTypeOf } from 'vitest';
import type {
  Adapter,
  InferResponse,
  ChatSession,
  TrainingJob
} from '@/api/domain-types';

describe('domain-types', () => {
  describe('Adapter type', () => {
    it('has camelCase fields', () => {
      const adapter: Adapter = {} as Adapter;
      // These should exist (camelCase)
      expectTypeOf(adapter).toHaveProperty('adapterId');
      expectTypeOf(adapter).toHaveProperty('tenantId');
      expectTypeOf(adapter).toHaveProperty('createdAt');
    });

    it('does not have snake_case fields', () => {
      const adapter: Adapter = {} as Adapter;
      // @ts-expect-error - snake_case should not exist
      adapter.adapter_id;
    });
  });

  describe('InferResponse type', () => {
    it('has camelCase inference fields', () => {
      const response: InferResponse = {} as InferResponse;
      expectTypeOf(response).toHaveProperty('tokensGenerated');
      expectTypeOf(response).toHaveProperty('latencyMs');
      expectTypeOf(response).toHaveProperty('adaptersUsed');
    });
  });

  describe('ChatSession type', () => {
    it('has camelCase session fields', () => {
      const session: ChatSession = {} as ChatSession;
      expectTypeOf(session).toHaveProperty('sessionId');
      expectTypeOf(session).toHaveProperty('tenantId');
      expectTypeOf(session).toHaveProperty('createdAt');
    });
  });

  describe('TrainingJob type', () => {
    it('has camelCase training fields', () => {
      const job: TrainingJob = {} as TrainingJob;
      expectTypeOf(job).toHaveProperty('jobId');
      expectTypeOf(job).toHaveProperty('datasetId');
      expectTypeOf(job).toHaveProperty('startedAt');
    });
  });
});
