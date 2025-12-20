/**
 * API Services Index
 *
 * This module provides:
 * 1. Domain-specific service instances (auth, adapters, training, etc.)
 * 2. A backward-compatible apiClient proxy that delegates to services
 *
 * Usage:
 * - New code: import { auth, adapters } from '@/api/services'
 * - Legacy code: import { apiClient } from '@/api/services' (or '@/api/client')
 */

import { ApiClient } from '@/api/client';
import { AuthService } from './auth';
import { SystemService } from './system';
import { AdaptersService } from './adapters';
import { StacksService } from './stacks';
import { TrainingService } from './training';
import { ChatService } from './chat';
import { DocumentsService } from './documents';
import { PoliciesService } from './policies';
import { ModelsService } from './models';
import { RoutingService } from './routing';
import { ReplayService } from './replay';
import { AdminService } from './admin';
import { FederationService } from './federation';
import { PluginsService } from './plugins';
import { ReposService } from './repos';
import { CodeService } from './code';
import { MonitoringService } from './monitoring';
import { InferenceService } from './inference';

// Create the base client with infrastructure (request, token management, retry logic)
const baseClient = new ApiClient();

// Create domain service instances
export const auth = new AuthService(baseClient);
export const system = new SystemService(baseClient);
export const adapters = new AdaptersService(baseClient);
export const stacks = new StacksService(baseClient);
export const training = new TrainingService(baseClient);
export const chat = new ChatService(baseClient);
export const documents = new DocumentsService(baseClient);
export const policies = new PoliciesService(baseClient);
export const models = new ModelsService(baseClient);
export const routing = new RoutingService(baseClient);
export const replay = new ReplayService(baseClient);
export const admin = new AdminService(baseClient);
export const federation = new FederationService(baseClient);
export const plugins = new PluginsService(baseClient);
export const repos = new ReposService(baseClient);
export const code = new CodeService(baseClient);
export const monitoring = new MonitoringService(baseClient);
export const inference = new InferenceService(baseClient);

// All services for proxy delegation
const services = [
  auth, system, adapters, stacks, training, chat, documents,
  policies, models, routing, replay, admin, federation, plugins,
  repos, code, monitoring, inference
] as const;

/**
 * Backward-compatible apiClient proxy.
 *
 * Delegates method calls to the appropriate domain service.
 * This allows existing code using apiClient.someMethod() to continue working
 * while methods are now organized in domain services.
 *
 * Example:
 *   apiClient.login() -> auth.login()
 *   apiClient.listAdapters() -> adapters.listAdapters()
 *   apiClient.createChatSession() -> chat.createChatSession()
 */
export const apiClient = new Proxy(baseClient, {
  get(target, prop: string | symbol, receiver) {
    // First check if it's a property on the base client (request, setToken, etc.)
    if (prop in target) {
      const value = Reflect.get(target, prop, receiver);
      if (typeof value === 'function') {
        return value.bind(target);
      }
      return value;
    }

    // Then check each service for the method
    for (const service of services) {
      const svc = service as unknown as Record<string | symbol, unknown>;
      if (prop in service && typeof svc[prop] === 'function') {
        return (svc[prop] as (...args: unknown[]) => unknown).bind(service);
      }
    }

    // Fall back to base client
    return Reflect.get(target, prop, receiver);
  }
}) as ApiClient &
  Omit<AuthService, 'client'> &
  Omit<SystemService, 'client'> &
  Omit<AdaptersService, 'client'> &
  Omit<StacksService, 'client'> &
  Omit<TrainingService, 'client'> &
  Omit<ChatService, 'client'> &
  Omit<DocumentsService, 'client'> &
  Omit<PoliciesService, 'client'> &
  Omit<ModelsService, 'client'> &
  Omit<RoutingService, 'client'> &
  Omit<ReplayService, 'client'> &
  Omit<AdminService, 'client'> &
  Omit<FederationService, 'client'> &
  Omit<PluginsService, 'client'> &
  Omit<ReposService, 'client'> &
  Omit<CodeService, 'client'> &
  Omit<MonitoringService, 'client'> &
  Omit<InferenceService, 'client'>;

// Re-export base client class for type usage
export { ApiClient } from '@/api/client';

// Default export for compatibility
export default apiClient;
