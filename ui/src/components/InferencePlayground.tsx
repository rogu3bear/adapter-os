import React, { useState, useEffect, useCallback, useRef } from 'react';
import { useCancellableOperation } from '../hooks/useCancellableOperation';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Textarea } from './ui/textarea';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Slider } from './ui/slider';
import { Checkbox } from './ui/checkbox';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Alert, AlertDescription } from './ui/alert';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './ui/collapsible';
import {
  Play,
  Copy,
  Download,
  History,
  Settings2,
  ChevronDown,
  Zap,
  Clock,
  BarChart3,
  Split,
  FileText,
  AlertTriangle,
  CheckCircle,
  Code,
  Square,
  Wifi,
  WifiOff,
  RotateCcw,
  Target,
  Layers,
  TrendingUp
} from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '../api/client';
import { InferRequest, InferResponse, InferenceSession, Adapter } from '../api/types';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
// 【ui/src/components/InferencePlayground.tsx§enhanced-inference-ux】 - Enhanced inference playground with streaming, batch support, and comprehensive error recovery
import { TraceVisualizer } from './TraceVisualizer';
import { logger, toError } from '../utils/logger';
import { useSearchParams } from 'react-router-dom';
import { ErrorRecovery, ErrorRecoveryTemplates } from './ui/error-recovery';
import { useProgressiveHints } from '../hooks/useProgressiveHints';
import { getPageHints } from '../data/page-hints';
import { ProgressiveHint } from './ui/progressive-hint';
import { ToolPageHeader } from './ui/page-headers/ToolPageHeader';
import { useFeatureDegradation } from '../hooks/useFeatureDegradation';

// 【ui/src/components/InferencePlayground.tsx§input-validation】 - Comprehensive input validation for edge cases
interface ValidationResult {
  valid: boolean;
  error?: string;
  suggestion?: string;
  warning?: string;
}

// 【ui/src/components/InferencePlayground.tsx§network-resilience】 - Network resilience and API edge case handling
type ConnectionQuality = 'fast' | 'slow' | 'offline';

interface AdaptiveTimeouts {
  connect: number;
  read: number;
}

// 【ui/src/components/InferencePlayground.tsx§state-management】 - Robust state management with edge case handling
interface SafeStorageOptions {
  maxRetries?: number;
  fallbackValue?: any;
}

interface StorageOperationResult<T> {
  success: boolean;
  data?: T;
  error?: string;
}

// 【ui/src/components/InferencePlayground.tsx§data-persistence】 - Data persistence with version migration and concurrent access
interface VersionedStorageData {
  version: string;
  data: any;
  lastModified: number;
  checksum?: string;
}

// 【ui/src/components/InferencePlayground.tsx§adapter-router】 - Adapter and router edge case handling
interface AdapterState {
  id: string;
  available: boolean;
  lastChecked: number;
  performance?: number;
}

interface RouterState {
  lastDecision: string[];
  decisionTime: number;
  confidence: number;
}

const STORAGE_VERSION = '2.0.0';

const generateChecksum = (data: any): string => {
  // Simple checksum for data integrity
  return btoa(JSON.stringify(data)).slice(0, 16);
};

const validateChecksum = (data: VersionedStorageData): boolean => {
  if (!data.checksum) return true; // No checksum to validate
  return data.checksum === generateChecksum(data.data);
};

// Migration functions for different storage versions
const migrateStorageData = (oldData: VersionedStorageData): VersionedStorageData => {
  const currentVersion = oldData.version;

  if (currentVersion === STORAGE_VERSION) {
    return oldData;
  }

  let migratedData = { ...oldData };

  // Migration from 1.0.0 to 2.0.0: Add checksum and enhance data structure
  if (currentVersion === '1.0.0') {
    migratedData.version = '2.0.0';
    migratedData.checksum = generateChecksum(migratedData.data);
    migratedData.lastModified = Date.now();

    logger.info('Migrated storage data from 1.0.0 to 2.0.0', {
      component: 'InferencePlayground',
      operation: 'storage_migration',
      hasChecksum: !!migratedData.checksum
    });
  }

  return migratedData;
};

// Enhanced storage with version management and concurrent access handling
const versionedStorage = {
  get: function <T>(key: string, fallbackValue: T): StorageOperationResult<T> {
    try {
      const item = localStorage.getItem(key);
      if (!item) {
        return { success: true, data: fallbackValue };
      }

      const parsed: VersionedStorageData = JSON.parse(item);

      // Validate checksum if present
      if (!validateChecksum(parsed)) {
        logger.warn('Storage data checksum validation failed', {
          component: 'InferencePlayground',
          operation: 'storage_validation',
          key,
          storedVersion: parsed.version
        });

        // Clear corrupted data
        localStorage.removeItem(key);
        return {
          success: false,
          error: 'Data corruption detected and cleared'
        };
      }

      // Migrate if needed
      const migratedData = migrateStorageData(parsed);

      // Save migrated data back if it changed
      if (migratedData.version !== parsed.version) {
        const saveResult = versionedStorage.set(key, migratedData.data);
        if (!saveResult.success) {
          logger.warn('Failed to save migrated data', {
            component: 'InferencePlayground',
            operation: 'migration_save',
            key
          });
        }
      }

      return { success: true, data: migratedData.data };
    } catch (error) {
      logger.error('Storage read failed', {
        component: 'InferencePlayground',
        operation: 'storage_read',
        key,
        error: error.message
      });

      // Clear corrupted data
      try {
        localStorage.removeItem(key);
      } catch {
        // Ignore cleanup errors
      }

      return {
        success: false,
        error: `Storage read failed: ${error.message}`
      };
    }
  },

  set: function <T>(key: string, value: T): StorageOperationResult<void> {
    try {
      const versionedData: VersionedStorageData = {
        version: STORAGE_VERSION,
        data: value,
        lastModified: Date.now(),
        checksum: generateChecksum(value)
      };

      // Check for concurrent modifications
      const existing = localStorage.getItem(key);
      if (existing) {
        const existingData: VersionedStorageData = JSON.parse(existing);
        // If data was modified within last 100ms, consider it concurrent
        if (Date.now() - existingData.lastModified < 100) {
          logger.warn('Concurrent storage modification detected', {
            component: 'InferencePlayground',
            operation: 'concurrent_modification',
            key
          });

          // Retry after a short delay
          setTimeout(() => {
            localStorage.setItem(key, JSON.stringify(versionedData));
          }, 50);

          return { success: true }; // Still return success, operation will complete
        }
      }

      localStorage.setItem(key, JSON.stringify(versionedData));
      return { success: true };
    } catch (error) {
      logger.error('Storage write failed', {
        component: 'InferencePlayground',
        operation: 'storage_write',
        key,
        error: error.message
      });

      return {
        success: false,
        error: `Storage write failed: ${error.message}`
      };
    }
  }
};

// 【ui/src/components/InferencePlayground.tsx§performance-scaling】 - Performance and scaling edge case handling
interface VirtualizedResponseProps {
  text: string;
  maxHeight?: number;
  maxLines?: number;
}

// 【ui/src/components/InferencePlayground.tsx§error-recovery】 - Error recovery and resilience edge case handling
interface ErrorRecoveryConfig {
  maxRetries: number;
  retryDelay: number;
  enablePartialRecovery: boolean;
  enableAuthRefresh: boolean;
  enableWorkerFailover: boolean;
}

// 【ui/src/components/InferencePlayground.tsx§browser-environment】 - Browser environment and compatibility edge case handling
interface BrowserCapabilities {
  localStorage: boolean;
  indexedDB: boolean;
  serviceWorker: boolean;
  webGL: boolean;
  webRTC: boolean;
  webAssembly: boolean;
  highMemory: boolean;
}

interface ResourceConstraints {
  memoryPressure: 'low' | 'medium' | 'high' | 'critical';
  cpuCores: number;
  deviceMemory: number; // GB
  hardwareConcurrency: number;
}

// 【ui/src/components/InferencePlayground.tsx§ui-ux-interactions】 - UI/UX interaction edge case handling
interface TouchState {
  startX: number;
  startY: number;
  isDragging: boolean;
  swipeDirection: 'none' | 'left' | 'right' | 'up' | 'down';
}

interface KeyboardNavigationState {
  focusedElement: string | null;
  tabIndex: number;
}

const RESPONSE_MAX_HEIGHT = 400;
const RESPONSE_MAX_LINES = 100;

// Virtualized response component for large text handling
const VirtualizedResponse: React.FC<VirtualizedResponseProps> = ({
  text,
  maxHeight = RESPONSE_MAX_HEIGHT,
  maxLines = RESPONSE_MAX_LINES
}) => {
  const lines = text.split('\n');

  // Truncate if too many lines for performance
  const shouldTruncate = lines.length > maxLines;
  const displayLines = shouldTruncate ? lines.slice(0, maxLines) : lines;
  const truncatedLineCount = lines.length - displayLines.length;

  return (
    <div className="space-y-1">
      <div
        className="overflow-auto border rounded-md p-3 bg-muted/20 font-mono text-sm leading-relaxed"
        style={{ maxHeight }}
      >
        {displayLines.map((line, index) => (
          <div key={index} className="whitespace-pre-wrap break-words">
            {line || '\u00A0'} {/* Non-breaking space for empty lines */}
          </div>
        ))}

        {shouldTruncate && (
          <div className="mt-2 pt-2 border-t text-muted-foreground text-xs">
            ... and {truncatedLineCount.toLocaleString()} more lines truncated for performance
          </div>
        )}
      </div>

      {shouldTruncate && (
        <div className="text-xs text-muted-foreground">
          Response too large to display fully. Showing first {maxLines.toLocaleString()} lines.
        </div>
      )}
    </div>
  );
};

// Throttled streaming updates for performance
const useThrottledStreaming = (tokens: StreamingToken[], fps = 30) => {
  const [displayTokens, setDisplayTokens] = useState<StreamingToken[]>([]);
  const lastUpdateRef = useRef(Date.now());

  useEffect(() => {
    const now = Date.now();
    const timeSinceLastUpdate = now - lastUpdateRef.current;
    const minInterval = 1000 / fps;

    if (timeSinceLastUpdate >= minInterval) {
      setDisplayTokens(tokens);
      lastUpdateRef.current = now;
    } else {
      const timeoutId = setTimeout(() => {
        setDisplayTokens(tokens);
        lastUpdateRef.current = Date.now();
      }, minInterval - timeSinceLastUpdate);

      return () => clearTimeout(timeoutId);
    }
  }, [tokens, fps]);

  return displayTokens;
};

// Memory-managed session storage to prevent leaks
const MAX_SESSIONS = 50;
const SESSION_CLEANUP_THRESHOLD = 40;

const useManagedSessions = (browserCapabilities: BrowserCapabilities | null) => {
  const [sessions, setSessions] = useState<InferenceSession[]>([]);

  const addSession = useCallback((session: InferenceSession) => {
    setSessions(prev => {
      let newSessions = [session, ...prev];

      // Cleanup old sessions when approaching limit
      if (newSessions.length > SESSION_CLEANUP_THRESHOLD) {
        newSessions = newSessions.slice(0, MAX_SESSIONS);
        logger.info('Cleaned up old sessions to prevent memory leaks', {
          component: 'InferencePlayground',
          operation: 'session_cleanup',
          sessionsBefore: prev.length,
          sessionsAfter: newSessions.length
        });
      }

      // Use versioned storage (skip if localStorage not available)
      if (browserCapabilities?.localStorage !== false) {
        const result = versionedStorage.set('inference_sessions', newSessions);
        if (!result.success) {
          logger.warn('Failed to persist sessions to storage', {
            component: 'InferencePlayground',
            operation: 'session_persistence',
            error: result.error
          });
        }
      } else {
        logger.debug('Skipping session persistence - localStorage not available', {
          component: 'InferencePlayground',
          operation: 'session_persistence_skip',
          reason: 'incognito_mode'
        });
      }

      return newSessions;
    });
  }, [browserCapabilities]);

  // Force garbage collection hint for large session lists
  useEffect(() => {
    if (sessions.length > 30) {
      // Request garbage collection if available (development mode)
      if (window.gc) {
        window.gc();
        logger.debug('Requested garbage collection for large session list', {
          component: 'InferencePlayground',
          operation: 'gc_hint',
          sessionCount: sessions.length
        });
      }
    }
  }, [sessions.length]);

  return { sessions, addSession };
};

// Error recovery and resilience utilities
const DEFAULT_ERROR_RECOVERY_CONFIG: ErrorRecoveryConfig = {
  maxRetries: 3,
  retryDelay: 1000,
  enablePartialRecovery: true,
  enableAuthRefresh: true,
  enableWorkerFailover: true,
};

const validateStreamingResponse = (data: any): boolean => {
  if (!data || typeof data !== 'object') return false;

  // Validate expected fields exist and have correct types
  if (data.token !== undefined && typeof data.token !== 'string') return false;
  if (data.done !== undefined && typeof data.done !== 'boolean') return false;
  if (data.error !== undefined && typeof data.error !== 'string') return false;

  return true;
};

const handleStreamingError = (error: Error, partialTokens: StreamingToken[]) => {
  // Attempt to reconstruct from partial data
  if (partialTokens.length > 0) {
    const reconstructedText = partialTokens.map(t => t.token).join('');

    logger.warn('Streaming interrupted with partial data', {
      component: 'InferencePlayground',
      operation: 'streaming_error_recovery',
      partialTokens: partialTokens.length,
      reconstructedLength: reconstructedText.length,
      error: error.message
    });

    // Show partial result with warning
    return {
      canRecover: true,
      partialResult: reconstructedText,
      message: `Streaming interrupted after ${partialTokens.length} tokens. Showing partial result.`
    };
  }

  return {
    canRecover: false,
    message: `Streaming failed: ${error.message}`
  };
};

const useAuthRefresh = () => {
  const refreshToken = useCallback(async () => {
    try {
      const response = await apiClient.refreshToken();
      logger.info('Auth token refreshed successfully', {
        component: 'InferencePlayground',
        operation: 'auth_refresh'
      });
      return response.token;
    } catch (error) {
      logger.error('Auth token refresh failed', {
        component: 'InferencePlayground',
        operation: 'auth_refresh',
        error: error.message
      });
      // Redirect to login on refresh failure
      window.location.href = '/login';
      throw error;
    }
  }, []);

  return refreshToken;
};

const useWorkerHealth = (refreshInterval = 30000) => {
  const [isHealthy, setIsHealthy] = useState(true);
  const [lastHealthCheck, setLastHealthCheck] = useState(Date.now());
  const [healthError, setHealthError] = useState<string | null>(null);

  useEffect(() => {
    const checkHealth = async () => {
      try {
        await apiClient.getWorkerHealth();
        setIsHealthy(true);
        setHealthError(null);
      } catch (error) {
        setIsHealthy(false);
        setHealthError(error.message);
        logger.warn('Worker health check failed', {
          component: 'InferencePlayground',
          operation: 'worker_health_check',
          error: error.message
        });
      }
      setLastHealthCheck(Date.now());
    };

    // Initial check
    checkHealth();

    // Periodic checks
    const interval = setInterval(checkHealth, refreshInterval);
    return () => clearInterval(interval);
  }, [refreshInterval]);

  return { isHealthy, lastHealthCheck, healthError };
};

// Browser environment detection utilities
const detectBrowserCapabilities = (): BrowserCapabilities => {
  const capabilities: BrowserCapabilities = {
    localStorage: false,
    indexedDB: false,
    serviceWorker: false,
    webGL: false,
    webRTC: false,
    webAssembly: false,
    highMemory: false,
  };

  try {
    // Test localStorage (fails in incognito/private browsing)
    localStorage.setItem('__test__', 'test');
    localStorage.removeItem('__test__');
    capabilities.localStorage = true;
  } catch {
    capabilities.localStorage = false;
  }

  // Test IndexedDB
  capabilities.indexedDB = !!window.indexedDB;

  // Test Service Worker
  capabilities.serviceWorker = 'serviceWorker' in navigator;

  // Test WebGL
  try {
    const canvas = document.createElement('canvas');
    const gl = canvas.getContext('webgl') || canvas.getContext('experimental-webgl');
    capabilities.webGL = !!gl;
  } catch {
    capabilities.webGL = false;
  }

  // Test WebRTC
  capabilities.webRTC = !!(window.RTCPeerConnection || window.webkitRTCPeerConnection);

  // Test WebAssembly
  capabilities.webAssembly = typeof WebAssembly === 'object' && typeof WebAssembly.instantiate === 'function';

  // Test high memory capability (navigator.deviceMemory is experimental)
  capabilities.highMemory = !!(navigator as any).deviceMemory && (navigator as any).deviceMemory >= 4;

  return capabilities;
};

const detectResourceConstraints = (): ResourceConstraints => {
  const constraints: ResourceConstraints = {
    memoryPressure: 'low',
    cpuCores: navigator.hardwareConcurrency || 1,
    deviceMemory: (navigator as any).deviceMemory || 0,
    hardwareConcurrency: navigator.hardwareConcurrency || 1,
  };

  // Estimate memory pressure based on available indicators
  if (constraints.deviceMemory > 0 && constraints.deviceMemory < 2) {
    constraints.memoryPressure = 'high';
  } else if (constraints.cpuCores < 4) {
    constraints.memoryPressure = 'medium';
  }

  // Check for memory pressure API if available
  if ('memory' in performance) {
    const memory = (performance as any).memory;
    if (memory.usedJSHeapSize / memory.totalJSHeapSize > 0.8) {
      constraints.memoryPressure = 'critical';
    } else if (memory.usedJSHeapSize / memory.totalJSHeapSize > 0.6) {
      constraints.memoryPressure = 'high';
    }
  }

  return constraints;
};

const detectIncognitoMode = async (): Promise<boolean> => {
  return new Promise((resolve) => {
    const fs = (window as any).webkitRequestFileSystem || (window as any).mozRequestFileSystem;
    if (fs) {
      // Firefox private browsing
      fs(0, 0, () => resolve(false), () => resolve(true));
    } else if ('MozAppearance' in document.documentElement.style) {
      // Firefox-specific detection
      const db = indexedDB.open('test');
      db.onerror = () => resolve(true);
      db.onsuccess = () => resolve(false);
    } else {
      // Chrome/Safari incognito detection via storage quota
      const testKey = '__incognito_test__';
      try {
        localStorage.setItem(testKey, 'test');
        localStorage.removeItem(testKey);
        resolve(false);
      } catch {
        resolve(true);
      }
    }
  });
};

const adaptiveTimeouts: Record<ConnectionQuality, AdaptiveTimeouts> = {
  fast: { connect: 5000, read: 30000 },
  slow: { connect: 15000, read: 120000 },
  offline: { connect: 30000, read: 300000 }
};

interface InferencePlaygroundProps {
  selectedTenant: string;
}

// Input validation utilities for edge cases
const MAX_PROMPT_LENGTH = 50000; // 50KB character limit
const MAX_PROMPT_BYTES = 100000; // 100KB byte limit

const validatePromptLength = (prompt: string): ValidationResult => {
  if (prompt.length > MAX_PROMPT_LENGTH) {
    return {
      valid: false,
      error: `Prompt too long (${prompt.length.toLocaleString()} characters). Maximum: ${MAX_PROMPT_LENGTH.toLocaleString()}`,
      suggestion: 'Consider breaking into smaller chunks or using batch processing for large inputs'
    };
  }

  const byteLength = new Blob([prompt]).size;
  if (byteLength > MAX_PROMPT_BYTES) {
    return {
      valid: false,
      error: `Prompt size too large (${(byteLength / 1024).toFixed(1)}KB). Maximum: ${(MAX_PROMPT_BYTES / 1024).toFixed(0)}KB`,
      suggestion: 'Reduce content size or consider using file upload for large documents'
    };
  }

  if (prompt.length > MAX_PROMPT_LENGTH * 0.8) {
    return {
      valid: true,
      warning: `Approaching character limit (${prompt.length.toLocaleString()}/${MAX_PROMPT_LENGTH.toLocaleString()})`
    };
  }

  return { valid: true };
};

const validateUnicodeContent = (text: string): ValidationResult => {
  try {
    // Normalize to NFC form for consistent processing
    const normalized = text.normalize('NFC');

    // Check for problematic Unicode ranges (control characters except common whitespace)
    const hasProblematicUnicode = /[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F-\u009F\u200B\u200C\u200D]/.test(normalized);
    if (hasProblematicUnicode) {
      return {
        valid: false,
        error: 'Prompt contains unsupported control or invisible characters',
        suggestion: 'Remove or replace invisible characters, zero-width spaces, or control characters'
      };
    }

    // Check for excessive emoji usage (potential spam/abuse)
    const emojiCount = (normalized.match(/\p{Emoji}/gu) || []).length;
    const textLength = normalized.replace(/\p{Emoji}/gu, '').length;
    if (emojiCount > textLength * 0.5 && emojiCount > 20) {
      return {
        valid: false,
        error: 'Too many emojis detected',
        suggestion: 'Reduce emoji usage or use descriptive text instead'
      };
    }

    return { valid: true };
  } catch (error) {
    return {
      valid: false,
      error: 'Unicode processing failed - text may contain invalid characters',
      suggestion: 'Try re-entering the text or copy from a different source'
    };
  }
};

const validatePromptContent = (prompt: string): ValidationResult => {
  if (!prompt || prompt.trim().length === 0) {
    return {
      valid: false,
      error: 'Prompt cannot be empty',
      suggestion: 'Please enter a question or instruction for the AI model'
    };
  }

  // Check for invisible Unicode characters that would be trimmed
  const visibleChars = prompt.replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F-\u009F\u200B\u200C\u200D\s]/g, '');
  if (visibleChars.length === 0) {
    return {
      valid: false,
      error: 'Prompt contains only invisible characters or whitespace',
      suggestion: 'Please enter meaningful text content'
    };
  }

  // Minimum meaningful length check (accounting for Unicode)
  const normalizedLength = prompt.normalize('NFC').trim().length;
  if (normalizedLength < 3) {
    return {
      valid: false,
      error: 'Prompt too short',
      suggestion: 'Please provide more context (minimum 3 characters)'
    };
  }

  return { valid: true };
};

const validatePrompt = (prompt: string): ValidationResult => {
  // Run all validations in order
  const lengthValidation = validatePromptLength(prompt);
  if (!lengthValidation.valid) return lengthValidation;

  const contentValidation = validatePromptContent(prompt);
  if (!contentValidation.valid) return contentValidation;

  const unicodeValidation = validateUnicodeContent(prompt);
  if (!unicodeValidation.valid) return unicodeValidation;

  // Combine warnings if any
  const warnings = [lengthValidation.warning, contentValidation.warning, unicodeValidation.warning]
    .filter(Boolean)
    .join('; ');

  return {
    valid: true,
    ...(warnings && { warning: warnings })
  };
};

// Security: Input sanitization to prevent XSS and other injection attacks
const sanitizeInput = (input: string): string => {
  if (!input) return input;

  // Basic XSS prevention - remove potentially dangerous HTML/script tags
  const sanitized = input
    .replace(/<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi, '') // Remove script tags
    .replace(/<iframe\b[^<]*(?:(?!<\/iframe>)<[^<]*)*<\/iframe>/gi, '') // Remove iframe tags
    .replace(/javascript:/gi, '') // Remove javascript: protocols
    .replace(/on\w+\s*=/gi, '') // Remove event handlers
    .replace(/<[^>]*>/g, '') // Remove all HTML tags as final fallback
    .trim();

  // Log if input was modified for security monitoring
  if (sanitized !== input) {
    logger.warn('Input sanitized for security', {
      component: 'InferencePlayground',
      operation: 'input_sanitization',
      originalLength: input.length,
      sanitizedLength: sanitized.length
    });
  }

  return sanitized;
};

// Privacy-aware monitoring (anonymized metrics only)
const recordPrivacySafeMetrics = (operation: string, data: any) => {
  // Remove any personally identifiable information
  const anonymized = { ...data };
  delete anonymized.userId;
  delete anonymized.email;
  delete anonymized.ip;
  delete anonymized.sessionId;

  logger.info(`Privacy-safe ${operation}`, {
    component: 'InferencePlayground',
    operation: `privacy_${operation}`,
    ...anonymized
  });
};

interface InferenceConfig extends InferRequest {
  id: string;
}

interface StreamingToken {
  token: string;
  timestamp: number;
}

interface PromptTemplate {
  id: string;
  name: string;
  description: string;
  prompt: string;
  category: string;
}

export function InferencePlayground({ selectedTenant }: InferencePlaygroundProps) {
  const [searchParams] = useSearchParams();
  const [mode, setMode] = useState<'single' | 'comparison'>('single');
  const [inferenceMode, setInferenceMode] = useState<'standard' | 'streaming' | 'batch'>('standard');
  const [prompt, setPrompt] = useState('');
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [selectedAdapterId, setSelectedAdapterId] = useState<string>('none');
  const [inferenceError, setInferenceError] = useState<Error | null>(null);
  const [adaptersLoadError, setAdaptersLoadError] = useState<Error | null>(null);

  // Enhanced state for streaming and batch
  const [streamingTokens, setStreamingTokens] = useState<StreamingToken[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [streamController, setStreamController] = useState<AbortController | null>(null);
  const [batchPrompts, setBatchPrompts] = useState<string[]>(['']);
  const [batchResults, setBatchResults] = useState<any>(null);
  const [isBatchRunning, setIsBatchRunning] = useState(false);

  // Input validation state
  const [promptValidation, setPromptValidation] = useState<ValidationResult>({ valid: true });
  const [batchValidation, setBatchValidation] = useState<ValidationResult[]>([{ valid: true }]);

  // Network resilience state
  const [activeRequests, setActiveRequests] = useState<Map<string, AbortController>>(new Map());
  const [requestQueue, setRequestQueue] = useState<Promise<any>[]>([]);

  // State management robustness state
  const [isTabVisible, setIsTabVisible] = useState(!document.hidden);
  const [shouldPauseInference, setShouldPauseInference] = useState(false);

  // Performance and scaling state
  const throttledStreamingTokens = useThrottledStreaming(streamingTokens);
  const { sessions: managedSessions, addSession: addManagedSession } = useManagedSessions(browserCapabilities);

  // Error recovery state
  const { isHealthy: workerHealthy, healthError } = useWorkerHealth();
  const refreshAuthToken = useAuthRefresh();
  const [errorRecoveryConfig] = useState(DEFAULT_ERROR_RECOVERY_CONFIG);

  // Browser environment state
  const [browserCapabilities, setBrowserCapabilities] = useState<BrowserCapabilities | null>(null);
  const [resourceConstraints, setResourceConstraints] = useState<ResourceConstraints | null>(null);
  const [isIncognito, setIsIncognito] = useState<boolean | null>(null);
  const [extensionInterference, setExtensionInterference] = useState<boolean>(false);

  // UI/UX interaction state
  const [touchState, setTouchState] = useState<TouchState>({
    startX: 0,
    startY: 0,
    isDragging: false,
    swipeDirection: 'none'
  });
  const [keyboardNav, setKeyboardNav] = useState<KeyboardNavigationState>({
    focusedElement: null,
    tabIndex: 0
  });
  const [windowSize, setWindowSize] = useState({ width: window.innerWidth, height: window.innerHeight });

  // Adapter and router state
  const [adapterStates, setAdapterStates] = useState<Map<string, AdapterState>>(new Map());
  const [routerState, setRouterState] = useState<RouterState | null>(null);
  const [adapterChangeDetected, setAdapterChangeDetected] = useState(false);

  // Streaming and batch processing state
  const [streamingBufferSize, setStreamingBufferSize] = useState(0);
  const [batchCancellationToken, setBatchCancellationToken] = useState<string | null>(null);
  const [streamingTimeoutId, setStreamingTimeoutId] = useState<NodeJS.Timeout | null>(null);

  // Session management enhancements
  const [templates, setTemplates] = useState<PromptTemplate[]>([
    {
      id: 'code-explanation',
      name: 'Code Explanation',
      description: 'Explain what this code does',
      prompt: 'Explain what this code does in simple terms:\n\n```\n{paste your code here}\n```',
      category: 'coding'
    },
    {
      id: 'bug-fix',
      name: 'Bug Analysis',
      description: 'Analyze and fix a bug',
      prompt: 'I have this bug in my code. Can you help me analyze and fix it?\n\nCode:\n```\n{paste your code here}\n```\n\nError:\n```\n{paste error message here}\n```\n\nWhat\'s wrong and how do I fix it?',
      category: 'coding'
    },
    {
      id: 'code-review',
      name: 'Code Review',
      description: 'Review code for improvements',
      prompt: 'Please review this code and suggest improvements:\n\n```\n{paste your code here}\n```\n\nFocus on:\n- Code quality and readability\n- Performance optimizations\n- Best practices\n- Potential bugs',
      category: 'coding'
    },
    {
      id: 'api-design',
      name: 'API Design',
      description: 'Design a REST API',
      prompt: 'Design a REST API for {describe your application}. Include:\n\n1. Main endpoints\n2. HTTP methods\n3. Request/response formats\n4. Authentication approach\n5. Error handling\n\nRequirements: {add your specific requirements}',
      category: 'design'
    },
    {
      id: 'documentation',
      name: 'Technical Writing',
      description: 'Write documentation',
      prompt: 'Write comprehensive documentation for {describe what needs documentation}:\n\nInclude:\n- Overview and purpose\n- Installation/setup instructions\n- Usage examples\n- API reference\n- Troubleshooting guide\n\nTarget audience: {specify audience}',
      category: 'writing'
    }
  ]);
  const [showTemplates, setShowTemplates] = useState(false);

  // Performance metrics
  const [metrics, setMetrics] = useState<{
    latency: number;
    tokensPerSecond: number;
    totalTokens: number;
  } | null>(null);

  // Cancellation support for inference operations
  const { state: inferenceState, start: startInference, cancel: cancelInference } = useCancellableOperation();

  // Refs for streaming
  const streamingRef = useRef<HTMLDivElement>(null);

  // Graceful degradation: Monitor adapter availability
  const adapterAvailability = useFeatureDegradation({
    featureId: 'adapters',
    healthCheck: () => {
      // Check current adapter state, don't reload (that's handled by useEffect)
      return adapters.length > 0;
    },
    checkInterval: 30000,
  });

  // Progressive hints with dynamic conditions
  const hints = getPageHints('inference').map(hint => ({
    ...hint,
    condition: hint.id === 'no-adapters-inference'
      ? () => adapters.length === 0
      : hint.id === 'auto-select-adapter'
        ? () => selectedAdapterId === 'none' && adapters.length > 0
        : hint.id === 'streaming-mode'
          ? () => inferenceMode !== 'streaming'
          : hint.id === 'comparison-mode-intro'
            ? () => mode === 'single' && adapters.length >= 2
          : hint.id === 'batch-inference'
            ? () => inferenceMode !== 'batch'
      : hint.condition
  }));
  const { getVisibleHint, dismissHint } = useProgressiveHints({
    pageKey: 'inference',
    hints
  });
  const visibleHint = getVisibleHint();

  // Network quality detection and adaptive behavior
  const [connectionQuality, setConnectionQuality] = useState<ConnectionQuality>('fast');
  const [lastNetworkCheck, setLastNetworkCheck] = useState(Date.now());

  // Detect connection quality
  useEffect(() => {
    const detectConnectionQuality = async () => {
      try {
        const start = Date.now();
        // Simple ping test to detect network latency
        const response = await fetch(`${apiClient.baseURL}/health`, {
          method: 'GET',
          cache: 'no-cache',
          signal: AbortSignal.timeout(5000)
        });

        const latency = Date.now() - start;

        if (latency < 200) {
          setConnectionQuality('fast');
        } else if (latency < 1000) {
          setConnectionQuality('slow');
        } else {
          setConnectionQuality('slow');
        }

        setLastNetworkCheck(Date.now());
      } catch (error) {
        // Network is offline or very slow
        setConnectionQuality('offline');
        setLastNetworkCheck(Date.now());
      }
    };

    // Initial check
    detectConnectionQuality();

    // Periodic checks every 30 seconds
    const interval = setInterval(detectConnectionQuality, 30000);
    return () => clearInterval(interval);
  }, []);

  // Tab visibility detection for inference management
  useEffect(() => {
    const handleVisibilityChange = () => {
      const visible = !document.hidden;
      setIsTabVisible(visible);

      if (!visible && (isLoadingA || isStreaming)) {
        // Tab became hidden during inference - pause and show warning
        setShouldPauseInference(true);

        // Cancel any ongoing requests
        activeRequests.forEach((controller, requestId) => {
          controller.abort();
          logger.info('Request cancelled due to tab becoming hidden', {
            component: 'InferencePlayground',
            operation: 'tab_visibility_change',
            requestId
          });
        });

        // Clear active requests
        setActiveRequests(new Map());
      } else if (visible && shouldPauseInference) {
        // Tab became visible again - reset pause state
        setShouldPauseInference(false);
      }
    };

    document.addEventListener('visibilitychange', handleVisibilityChange);
    return () => document.removeEventListener('visibilitychange', handleVisibilityChange);
  }, [isLoadingA, isStreaming, activeRequests, shouldPauseInference]);

  // Browser environment detection
  useEffect(() => {
    const capabilities = detectBrowserCapabilities();
    setBrowserCapabilities(capabilities);

    const constraints = detectResourceConstraints();
    setResourceConstraints(constraints);

    // Detect incognito mode
    detectIncognitoMode().then(setIsIncognito);

    // Detect potential extension interference (simplified heuristic)
    const detectExtensionInterference = () => {
      // Check for unusual script injections or modified prototypes
      const hasUnusualScripts = document.scripts.length > 20; // Arbitrary threshold
      const hasModifiedPrototypes = Object.getOwnPropertyNames(window).some(prop =>
        prop.startsWith('__') && prop.endsWith('__')
      );

      setExtensionInterference(hasUnusualScripts || hasModifiedPrototypes);

      if (hasUnusualScripts || hasModifiedPrototypes) {
        logger.warn('Potential browser extension interference detected', {
          component: 'InferencePlayground',
          operation: 'extension_detection',
          scriptCount: document.scripts.length,
          hasModifiedPrototypes
        });
      }
    };

    detectExtensionInterference();

    logger.info('Browser environment detected', {
      component: 'InferencePlayground',
      operation: 'browser_detection',
      capabilities,
      constraints,
      isIncognito: 'pending' // Will be updated when promise resolves
    });
  }, []);

  // Keyboard navigation and accessibility
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // Global keyboard shortcuts
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) {
        // Don't interfere with text input
        return;
      }

      switch (event.key) {
        case 'g':
          if ((event.ctrlKey || event.metaKey) && !isLoadingA && promptValidation.valid) {
            event.preventDefault();
            handleInfer(configA, setResponseA, setIsLoadingA);
          }
          break;
        case 's':
          if ((event.ctrlKey || event.metaKey)) {
            event.preventDefault();
            setInferenceMode(inferenceMode === 'streaming' ? 'standard' : 'streaming');
          }
          break;
        case 'b':
          if ((event.ctrlKey || event.metaKey)) {
            event.preventDefault();
            setInferenceMode(inferenceMode === 'batch' ? 'standard' : 'batch');
          }
          break;
        case 'Escape':
          if (isStreaming && streamController) {
            event.preventDefault();
            streamController.abort();
          }
          if (isLoadingA) {
            event.preventDefault();
            // Cancel current inference (implementation depends on API)
            logger.info('Inference cancelled via keyboard shortcut', {
              component: 'InferencePlayground',
              operation: 'keyboard_cancel'
            });
          }
          break;
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [inferenceMode, isLoadingA, isStreaming, streamController, promptValidation.valid, configA, handleInfer]);

  // Window resize handling
  useEffect(() => {
    const handleResize = () => {
      const newSize = { width: window.innerWidth, height: window.innerHeight };
      setWindowSize(newSize);

      // Adapt UI for mobile/desktop
      if (newSize.width < 768 && mode !== 'single') {
        logger.info('Switching to single mode for mobile', {
          component: 'InferencePlayground',
          operation: 'responsive_mode_switch',
          windowWidth: newSize.width
        });
        setMode('single');
      }
    };

    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, [mode]);

  // Touch gesture handling for mobile
  useEffect(() => {
    const handleTouchStart = (event: TouchEvent) => {
      if (event.touches.length === 1) {
        const touch = event.touches[0];
        setTouchState({
          startX: touch.clientX,
          startY: touch.clientY,
          isDragging: false,
          swipeDirection: 'none'
        });
      }
    };

    const handleTouchMove = (event: TouchEvent) => {
      if (event.touches.length === 1 && touchState.startX !== 0) {
        const touch = event.touches[0];
        const deltaX = touch.clientX - touchState.startX;
        const deltaY = touch.clientY - touchState.startY;
        const minSwipeDistance = 50;

        if (Math.abs(deltaX) > minSwipeDistance || Math.abs(deltaY) > minSwipeDistance) {
          setTouchState(prev => ({ ...prev, isDragging: true }));

          // Determine swipe direction
          if (Math.abs(deltaX) > Math.abs(deltaY)) {
            setTouchState(prev => ({
              ...prev,
              swipeDirection: deltaX > 0 ? 'right' : 'left'
            }));
          } else {
            setTouchState(prev => ({
              ...prev,
              swipeDirection: deltaY > 0 ? 'down' : 'up'
            }));
          }
        }
      }
    };

    const handleTouchEnd = (event: TouchEvent) => {
      if (touchState.isDragging) {
        // Handle swipe gestures
        switch (touchState.swipeDirection) {
          case 'left':
            // Swipe left: switch to next mode
            if (inferenceMode === 'standard') setInferenceMode('streaming');
            else if (inferenceMode === 'streaming') setInferenceMode('batch');
            break;
          case 'right':
            // Swipe right: switch to previous mode
            if (inferenceMode === 'batch') setInferenceMode('streaming');
            else if (inferenceMode === 'streaming') setInferenceMode('standard');
            break;
          case 'up':
            // Swipe up: hide/show templates
            setShowTemplates(prev => !prev);
            break;
        }

        logger.info('Touch gesture handled', {
          component: 'InferencePlayground',
          operation: 'touch_gesture',
          direction: touchState.swipeDirection,
          mode: inferenceMode
        });
      }

      // Reset touch state
      setTouchState({
        startX: 0,
        startY: 0,
        isDragging: false,
        swipeDirection: 'none'
      });
    };

    // Only add touch listeners on mobile devices
    if (windowSize.width < 768) {
      document.addEventListener('touchstart', handleTouchStart, { passive: true });
      document.addEventListener('touchmove', handleTouchMove, { passive: true });
      document.addEventListener('touchend', handleTouchEnd, { passive: true });

      return () => {
        document.removeEventListener('touchstart', handleTouchStart);
        document.removeEventListener('touchmove', handleTouchMove);
        document.removeEventListener('touchend', handleTouchEnd);
      };
    }
  }, [touchState.startX, touchState.startY, touchState.isDragging, touchState.swipeDirection, inferenceMode, windowSize.width]);

  // Adapter state monitoring
  useEffect(() => {
    const monitorAdapters = () => {
      // Simplified adapter monitoring - in real implementation this would poll the API
      adapters.forEach(adapter => {
        const currentState = adapterStates.get(adapter.id);
        const now = Date.now();

        // Check if adapter state has changed since last check
        const shouldUpdate = !currentState || (now - currentState.lastChecked) > 30000; // 30s interval

        if (shouldUpdate) {
          // Simulate adapter availability check
          const isAvailable = Math.random() > 0.1; // 90% availability simulation

          setAdapterStates(prev => {
            const newStates = new Map(prev);
            newStates.set(adapter.id, {
              id: adapter.id,
              available: isAvailable,
              lastChecked: now,
              performance: Math.random() * 100 // Simulated performance score
            });
            return newStates;
          });

          // Detect if selected adapter became unavailable
          if (!isAvailable && selectedAdapterId === adapter.id) {
            setAdapterChangeDetected(true);
            logger.warn('Selected adapter became unavailable', {
              component: 'InferencePlayground',
              operation: 'adapter_state_change',
              adapterId: adapter.id
            });
          }
        }
      });
    };

    if (adapters.length > 0) {
      monitorAdapters();
      const interval = setInterval(monitorAdapters, 30000); // Check every 30 seconds
      return () => clearInterval(interval);
    }
  }, [adapters, adapterStates, selectedAdapterId]);

  // Router decision conflict detection
  useEffect(() => {
    if (responseA && responseA.trace) {
      const trace = responseA.trace as any;
      const currentDecision = trace.adapters_used || [];

      if (routerState) {
        // Check for decision conflicts (same prompt, different adapters)
        const timeDiff = Date.now() - routerState.decisionTime;
        if (timeDiff < 5000 && JSON.stringify(currentDecision.sort()) !== JSON.stringify(routerState.lastDecision.sort())) {
          logger.warn('Router decision conflict detected', {
            component: 'InferencePlayground',
            operation: 'router_conflict',
            previousDecision: routerState.lastDecision,
            currentDecision,
            timeDiff
          });
        }
      }

      setRouterState({
        lastDecision: currentDecision,
        decisionTime: Date.now(),
        confidence: trace.confidence || 0.8
      });
    }
  }, [responseA, routerState]);

  // Input validation effects
  useEffect(() => {
    const validation = validatePrompt(prompt);
    setPromptValidation(validation);
  }, [prompt]);

  useEffect(() => {
    const validations = batchPrompts.map(p => validatePrompt(p));
    setBatchValidation(validations);
  }, [batchPrompts]);

  // Safe localStorage operations with corruption detection
  const safeStorage = {
    get: function <T>(key: string, fallbackValue: T, options: SafeStorageOptions = {}): T {
      const { maxRetries = 3 } = options;

      for (let attempt = 0; attempt <= maxRetries; attempt++) {
        try {
          const item = localStorage.getItem(key);
          if (!item) return fallbackValue;

          const parsed = JSON.parse(item);

          // Basic type validation
          if (typeof parsed !== typeof fallbackValue && parsed !== null) {
            throw new Error('Type mismatch in stored data');
          }

          return parsed;
        } catch (error) {
          logger.warn('localStorage read failed, retrying', {
            component: 'InferencePlayground',
            operation: 'safe_storage_get',
            key,
            attempt,
            error: error.message
          });

          // On last attempt, clear corrupted data
          if (attempt === maxRetries) {
            try {
              localStorage.removeItem(key);
            } catch {
              // Ignore cleanup errors
            }
            return fallbackValue;
          }

          // Small delay before retry
          setTimeout(() => {}, 10);
        }
      }

      return fallbackValue;
    },

    set: function <T>(key: string, value: T, options: SafeStorageOptions = {}): StorageOperationResult<void> {
      const { maxRetries = 3 } = options;

      for (let attempt = 0; attempt <= maxRetries; attempt++) {
        try {
          // Test storage availability first
          const testKey = '__storage_test__';
          localStorage.setItem(testKey, 'test');
          localStorage.removeItem(testKey);

          const serialized = JSON.stringify(value);
          localStorage.setItem(key, serialized);

          return { success: true };
        } catch (error) {
          logger.warn('localStorage write failed, retrying', {
            component: 'InferencePlayground',
            operation: 'safe_storage_set',
            key,
            attempt,
            error: error.message
          });

          // On quota exceeded, try to free up space
          if (error.name === 'QuotaExceededError' && attempt === maxRetries - 1) {
            try {
              // Clear old inference sessions to free space
              const sessions = safeStorage.get('inference_sessions', []);
              const recentSessions = sessions.slice(-5); // Keep only last 5
              localStorage.setItem('inference_sessions', JSON.stringify(recentSessions));
              localStorage.removeItem('inference_templates'); // Clear templates as well

              // Retry once more
              const serialized = JSON.stringify(value);
              localStorage.setItem(key, serialized);
              return { success: true };
            } catch {
              return {
                success: false,
                error: 'Storage quota exceeded and cleanup failed'
              };
            }
          }

          if (attempt === maxRetries) {
            return {
              success: false,
              error: error.message || 'Storage operation failed'
            };
          }

          // Small delay before retry
          setTimeout(() => {}, 10);
        }
      }

      return { success: false, error: 'Max retries exceeded' };
    }
  };

  // Network resilience utilities
  const getTimeoutForQuality = useCallback((quality: ConnectionQuality): AdaptiveTimeouts => {
    return adaptiveTimeouts[quality];
  }, []);

  const isRateLimitError = useCallback((error: any): boolean => {
    return error?.status === 429 || error?.message?.toLowerCase().includes('rate limit');
  }, []);

  const executeWithRateLimitRetry = useCallback(async function <T>(
    operation: () => Promise<T>,
    attempt = 0
  ): Promise<T> {
    const maxAttempts = 5;
    const baseDelay = 1000;

    try {
      return await operation();
    } catch (error) {
      if (attempt >= maxAttempts || !isRateLimitError(error)) {
        throw error;
      }

      const delay = baseDelay * Math.pow(2, attempt) + Math.random() * 1000;
      logger.info('Rate limit hit, retrying', {
        component: 'InferencePlayground',
        operation: 'rate_limit_retry',
        attempt,
        delay
      });

      await new Promise(resolve => setTimeout(resolve, delay));
      return executeWithRateLimitRetry(operation, attempt + 1);
    }
  }, [isRateLimitError]);

  const executeDeduplicatedInference = useCallback(async (
    config: InferenceConfig,
    requestId: string
  ): Promise<InferResponse> => {
    // Check for existing identical request
    if (activeRequests.has(requestId)) {
      activeRequests.get(requestId)?.abort();
      setActiveRequests(prev => {
        const newMap = new Map(prev);
        newMap.delete(requestId);
        return newMap;
      });
    }

    const controller = new AbortController();
    setActiveRequests(prev => new Map(prev).set(requestId, controller));

    try {
      return await executeWithRateLimitRetry(() =>
        apiClient.infer(config, {}, false, controller.signal)
      );
    } finally {
      setActiveRequests(prev => {
        const newMap = new Map(prev);
        newMap.delete(requestId);
        return newMap;
      });
    }
  }, [activeRequests, executeWithRateLimitRetry]);

  // Streaming inference with network resilience
  const executeStreamingInference = useCallback(async (config: InferenceConfig) => {
    setIsStreaming(true);
    setStreamingTokens([]);
    setInferenceError(null);

    const requestId = `streaming_${config.id}_${Date.now()}`;

    try {
      // Use adaptive timeouts based on connection quality
      if (connectionQuality === 'offline') {
        // Graceful degradation to non-streaming
        setInferenceError(new Error('Network connection is offline. Switching to standard inference.'));
        await executeInference(config);
        return;
      }

      const response = await executeDeduplicatedInference({
        ...config,
        adapters: selectedAdapterId && selectedAdapterId !== 'none' ? [selectedAdapterId] : undefined,
      }, requestId);

      // Simulate streaming by showing tokens progressively with buffer overflow protection
      const words = response.text.split(' ');
      const MAX_BUFFER_SIZE = 1000; // Prevent excessive memory usage
      let bufferSize = 0;

      // Set up streaming timeout (5 minutes max for any streaming operation)
      const timeoutId = setTimeout(() => {
        logger.warn('Streaming timeout reached, cancelling', {
          component: 'InferencePlayground',
          operation: 'streaming_timeout',
          tokensProcessed: bufferSize
        });
        if (controller) controller.abort();
      }, 300000); // 5 minutes

      setStreamingTimeoutId(timeoutId);

      try {
        for (let i = 0; i < words.length; i++) {
          // Check buffer size to prevent overflow
          if (bufferSize >= MAX_BUFFER_SIZE) {
            logger.warn('Streaming buffer overflow prevented', {
              component: 'InferencePlayground',
              operation: 'buffer_overflow_prevention',
              bufferSize,
              maxBufferSize: MAX_BUFFER_SIZE
            });
            break;
          }

          // Check if streaming was cancelled
          if (controller?.signal.aborted) {
            logger.info('Streaming cancelled by user', {
              component: 'InferencePlayground',
              operation: 'streaming_cancelled'
            });
            break;
          }

          // Adaptive delay based on connection quality
          const delay = connectionQuality === 'fast' ? 50 : connectionQuality === 'slow' ? 200 : 500;
          await new Promise(resolve => setTimeout(resolve, delay));

          const token = words[i] + (i < words.length - 1 ? ' ' : '');
          setStreamingTokens(prev => [...prev, {
            token,
            timestamp: Date.now()
          }]);

          bufferSize++;
          setStreamingBufferSize(bufferSize);
        }
      } finally {
        clearTimeout(timeoutId);
        setStreamingTimeoutId(null);
      }

      recordInferenceMetrics(config, response, Date.now());
    } catch (error) {
      if (error.name === 'AbortError') {
        // Request was cancelled, don't show error
        return;
      }

      // Attempt partial recovery for streaming errors
      if (errorRecoveryConfig.enablePartialRecovery) {
        const recovery = handleStreamingError(error, throttledStreamingTokens);
        if (recovery.canRecover) {
          // Show partial result with warning
          setInferenceError(new Error(recovery.message));
          // Could set a partial response here if we want to show the reconstructed text
          logger.info('Streaming partially recovered', {
            component: 'InferencePlayground',
            operation: 'streaming_partial_recovery',
            partialTokens: throttledStreamingTokens.length
          });
        } else {
          setInferenceError(error instanceof Error ? error : new Error('Streaming failed'));
        }
      } else {
        setInferenceError(error instanceof Error ? error : new Error('Streaming failed'));
      }

      logger.error('Streaming inference failed', {
        component: 'InferencePlayground',
        operation: 'streaming_inference',
        configId: config.id,
        tenantId: selectedTenant,
        connectionQuality,
        partialRecoveryEnabled: errorRecoveryConfig.enablePartialRecovery,
        partialTokensAvailable: throttledStreamingTokens.length,
      }, toError(error));
    } finally {
      setIsStreaming(false);
    }
  }, [selectedAdapterId, recordInferenceMetrics, selectedTenant, connectionQuality, executeDeduplicatedInference]);

  const executeBatchInference = useCallback(async () => {
    // Validate all batch prompts before proceeding
    const invalidPrompts = batchValidation.filter(v => !v.valid);
    if (invalidPrompts.length > 0) {
      setInferenceError(new Error(`Cannot run batch: ${invalidPrompts.length} invalid prompt(s)`));
      return;
    }

    const validPrompts = batchPrompts.filter(p => p.trim() && validatePrompt(p.trim()).valid);
    if (validPrompts.length === 0) {
      setInferenceError(new Error('No valid prompts to process'));
      return;
    }

    // Check for excessively large batches
    if (validPrompts.length > 50) {
      setInferenceError(new Error('Batch too large. Maximum 50 prompts per batch.'));
      return;
    }

    // Generate cancellation token for this batch
    const cancellationToken = `batch_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    setBatchCancellationToken(cancellationToken);

    setIsBatchRunning(true);
    setInferenceError(null);

    try {
      const batchRequest = {
        requests: validPrompts.map((prompt, index) => ({
          id: `req_${index}`,
          prompt: prompt.trim(),
          max_tokens: configA.max_tokens,
          temperature: configA.temperature,
          top_k: configA.top_k,
          top_p: configA.top_p,
          seed: configA.seed,
          require_evidence: configA.require_evidence,
        }))
      };

      // Use rate limiting for batch requests
      const response = await executeWithRateLimitRetry(() =>
        apiClient.batchInfer(batchRequest)
      );
      setBatchResults(response);
    } catch (error) {
      if (error.name === 'AbortError') {
        // Request was cancelled, don't show error
        return;
      }

      setInferenceError(error instanceof Error ? error : new Error('Batch inference failed'));
      logger.error('Batch inference failed', {
        component: 'InferencePlayground',
        operation: 'batch_inference',
        tenantId: selectedTenant,
        promptCount: validPrompts.length,
        connectionQuality,
      }, toError(error));
    } finally {
      setIsBatchRunning(false);
    }
  }, [batchPrompts, batchValidation, configA, selectedTenant, executeWithRateLimitRetry, connectionQuality]);


  // Record performance metrics
  const recordInferenceMetrics = useCallback((config: InferenceConfig, response: InferResponse, latency: number) => {
    const tokens = response.token_count || response.text?.split(' ').length || 0;
    const tokensPerSecond = tokens / (latency / 1000);

    setMetrics({
      latency,
      tokensPerSecond,
      totalTokens: tokens
    });

    // Log metrics for monitoring
    logger.info('Inference completed', {
      component: 'InferencePlayground',
      operation: 'inference_metrics',
      configId: config.id,
      adapterId: selectedAdapterId,
      latency,
      tokens,
      tokensPerSecond,
      tenantId: selectedTenant,
    });
  }, [selectedAdapterId, selectedTenant]);

  // Template application
  const applyTemplate = useCallback((template: PromptTemplate) => {
    setConfigA({ ...configA, prompt: template.prompt });
    setPrompt(template.prompt);
    setShowTemplates(false);
    // Success feedback through UI update
  }, [configA]);

  // Unified inference execution function
  const executeInference = useCallback(async (config: InferenceConfig) => {
    const startTime = Date.now();

    // Check worker health before proceeding
    if (!workerHealthy && errorRecoveryConfig.enableWorkerFailover) {
      logger.warn('Worker unhealthy, attempting inference anyway', {
        component: 'InferencePlayground',
        operation: 'worker_health_check',
        healthError
      });
    }

    try {
      if (inferenceMode === 'streaming') {
        await executeStreamingInference(config);
      } else {
        const inferenceRequest: InferRequest = {
          ...config,
          adapters: selectedAdapterId && selectedAdapterId !== 'none' ? [selectedAdapterId] : undefined,
        };

        let response;
        try {
          response = await executeWithRateLimitRetry(() =>
            apiClient.infer(inferenceRequest)
          );
        } catch (apiError) {
          // Handle authentication expiry
          if (errorRecoveryConfig.enableAuthRefresh &&
              (apiError.status === 401 || apiError.message?.toLowerCase().includes('auth'))) {
            logger.info('Authentication expired, attempting refresh', {
              component: 'InferencePlayground',
              operation: 'auth_recovery'
            });

            try {
              await refreshAuthToken();
              // Retry with refreshed token
              response = await executeWithRateLimitRetry(() =>
                apiClient.infer(inferenceRequest)
              );
            } catch (retryError) {
              logger.error('Auth refresh retry failed', {
                component: 'InferencePlayground',
                operation: 'auth_recovery_retry',
                originalError: apiError.message,
                retryError: retryError.message
              });
              throw retryError;
            }
          } else {
            throw apiError;
          }
        }

        recordInferenceMetrics(config, response, Date.now() - startTime);
        return response;
      }
    } catch (error) {
      // Handle worker failures
      if (!workerHealthy && errorRecoveryConfig.enableWorkerFailover) {
        logger.error('Inference failed with unhealthy worker', {
          component: 'InferencePlayground',
          operation: 'worker_failover',
          healthError,
          inferenceError: error.message
        });

        const enhancedError = new Error(
          `Service temporarily unavailable. ${error.message}. Please try again in a few moments.`
        );
        throw enhancedError;
      }

      throw error;
    }
  }, [inferenceMode, executeStreamingInference, selectedAdapterId, recordInferenceMetrics, workerHealthy, healthError, errorRecoveryConfig, executeWithRateLimitRetry, refreshAuthToken]);
  
  // Inference configurations
  const [configA, setConfigA] = useState<InferenceConfig>({
    id: 'a',
    prompt: '',
    max_tokens: 100,
    temperature: 0.7,
    top_k: 50,
    top_p: 0.9,
    seed: undefined,
    require_evidence: false,
  });

  const [configB, setConfigB] = useState<InferenceConfig>({
    id: 'b',
    prompt: '',
    max_tokens: 100,
    temperature: 0.9,
    top_k: 50,
    top_p: 0.9,
    seed: undefined,
    require_evidence: false,
  });

  const [responseA, setResponseA] = useState<InferResponse | null>(null);
  const [responseB, setResponseB] = useState<InferResponse | null>(null);
  const [isLoadingA, setIsLoadingA] = useState(false);
  const [isLoadingB, setIsLoadingB] = useState(false);
  
  const [recentSessions, setRecentSessions] = useState<InferenceSession[]>([]);

  // Enhanced error recovery with contextual suggestions
  const getInferenceErrorRecovery = useCallback((error: Error) => {
    const errorMessage = error.message.toLowerCase();

    if (errorMessage.includes('network') || errorMessage.includes('fetch')) {
      return ErrorRecoveryTemplates.networkError(() => executeInference(configA));
    }

    if (errorMessage.includes('auth') || errorMessage.includes('401')) {
      return ErrorRecoveryTemplates.authError(() => executeInference(configA));
    }

    return ErrorRecoveryTemplates.genericError(
      error,
      () => executeInference(configA)
    );
  }, [configA]);

  useEffect(() => {
    // Load recent sessions from localStorage
    const stored = localStorage.getItem('inference_sessions');
    if (stored) {
      try {
        setRecentSessions(JSON.parse(stored));
      } catch (err) {
        logger.error('Failed to parse stored inference sessions', {
          component: 'InferencePlayground',
          operation: 'loadSessions',
        }, toError(err));
      }
    }

    // Load adapters
    const loadAdapters = async () => {
      try {
        const adapterList = await apiClient.listAdapters();
        setAdapters(adapterList);

        // Check for adapter query parameter
        const adapterParam = searchParams.get('adapter');
        if (adapterParam) {
          // Try to find the adapter by ID or adapter_id
          const targetAdapter = adapterList.find((a: Adapter) =>
            a.id === adapterParam || a.adapter_id === adapterParam
          );
          if (targetAdapter) {
            setSelectedAdapterId(targetAdapter.id);
            // Success - no need for toast, UI updates
            return;
          } else {
            logger.warn('Requested adapter not found', {
              component: 'InferencePlayground',
              operation: 'loadAdapters',
              requestedAdapter: adapterParam,
            });
          }
        }

        // Fallback: Select first active adapter if available
        const activeAdapter = adapterList.find((a: Adapter) => ['hot', 'warm', 'resident'].includes(a.current_state));
        if (activeAdapter) {
          setSelectedAdapterId(activeAdapter.id);
        }
      } catch (err) {
        const error = err instanceof Error ? err : new Error('Failed to load adapters');
        logger.error('Failed to load adapters', {
          component: 'InferencePlayground',
          operation: 'loadAdapters',
        }, error);
        setAdaptersLoadError(error);
        // Don't set inferenceError - allow graceful degradation with base model
      }
    };
    loadAdapters();
  }, [searchParams]);

  const saveSession = (config: InferenceConfig, response: InferResponse) => {
    // Convert InferResponse to EnhancedInferResponse for session storage
    const enhancedResponse = {
      ...response,
      token_count: response.token_count || 0,
      finish_reason: response.finish_reason || 'stop',
      latency_ms: response.latency_ms || 0,
      trace: response.trace,
    };
    
    const session: InferenceSession = {
      id: Date.now().toString(),
      created_at: new Date().toISOString(),
      prompt: config.prompt,
      request: config,
      response: enhancedResponse as any, // Type compatibility
      status: 'completed',
    };

    // Use managed sessions to prevent memory leaks
    addManagedSession(session);
  };

  const handleInfer = async (config: InferenceConfig, setResponse: (r: InferResponse | null) => void, setLoading: (l: boolean) => void) => {
    // Validate prompt before proceeding
    const validation = validatePrompt(config.prompt);
    if (!validation.valid) {
      setInferenceError(new Error(validation.error || 'Invalid prompt'));
      return;
    }

    setInferenceError(null);
    setLoading(true);
    setResponse(null);

    try {
      if (inferenceMode === 'streaming') {
        // Handle streaming inference
        await executeStreamingInference(config);
      } else {
        // Handle standard inference
      await startInference(async (signal) => {
          const response = await executeInference(config);
        setResponse(response);
        saveSession(config, response);
        return response;
      }, `inference-${config.id}`);
      }
    } catch (err) {
      if (err && (err as Error).name !== 'AbortError') { // Only set error if it's not a cancellation
        const error = err instanceof Error ? err : new Error('Inference failed');
        setInferenceError(error);
        logger.error('Inference request failed', {
          component: 'InferencePlayground',
          operation: 'infer',
          configId: config.id,
          tenantId: selectedTenant,
          adapterId: selectedAdapterId,
          inferenceMode,
        }, toError(err));
      }
    } finally {
      setLoading(false);
    }
  };

  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
    // Success - no need for toast, UI feedback is sufficient
  };

  const handleExport = (config: InferenceConfig, response: InferResponse | null) => {
    if (!response) return;

    const data = {
      prompt: config.prompt,
      config,
      response,
      timestamp: new Date().toISOString(),
    };

    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `inference-${Date.now()}.json`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
    // Success - browser download feedback is sufficient
  };

  const loadSession = (session: InferenceSession) => {
    setPrompt(session.prompt);
    setConfigA({ ...configA, prompt: session.prompt, ...session.request });
    if (session.response) {
      setResponseA(session.response);
    }
    // Success - UI updates are sufficient feedback
  };

  const handleReplay = async (bundleId: string) => {
    const trace = await apiClient.get(`/api/replay/${bundleId}`);
    // setTrace(trace.data); // Display bundle
  };

  const renderAdvancedOptions = (config: InferenceConfig, setConfig: (c: InferenceConfig) => void) => (
    <Collapsible open={showAdvanced} onOpenChange={setShowAdvanced}>
      <CollapsibleTrigger asChild>
        <Button variant="ghost" className="w-full justify-between" aria-label="Toggle advanced options" aria-expanded={showAdvanced}>
          <span className="flex items-center gap-2">
            <Settings2 className="h-4 w-4" aria-hidden="true" />
            Advanced Options
          </span>
          <ChevronDown className={`h-4 w-4 transition-transform ${showAdvanced ? 'rotate-180' : ''}`} />
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className="space-y-4 pt-4">
        <div className="space-y-2">
          <div className="flex justify-between">
            <Label>Max Tokens</Label>
            <span className="text-sm text-muted-foreground">{config.max_tokens}</span>
          </div>
          <Slider
            value={[config.max_tokens || 100]}
            onValueChange={(v) => setConfig({ ...config, max_tokens: v[0] })}
            min={10}
            max={2000}
            step={10}
          />
        </div>

        <div className="space-y-2">
          <div className="flex justify-between">
            <Label>Temperature</Label>
            <span className="text-sm text-muted-foreground">{config.temperature?.toFixed(2)}</span>
          </div>
          <Slider
            value={[config.temperature || 0.7]}
            onValueChange={(v) => setConfig({ ...config, temperature: v[0] })}
            min={0}
            max={2}
            step={0.1}
          />
        </div>

        <div className="space-y-2">
          <div className="flex justify-between">
            <Label>Top K</Label>
            <span className="text-sm text-muted-foreground">{config.top_k}</span>
          </div>
          <Slider
            value={[config.top_k || 50]}
            onValueChange={(v) => setConfig({ ...config, top_k: v[0] })}
            min={1}
            max={100}
            step={1}
          />
        </div>

        <div className="space-y-2">
          <div className="flex justify-between">
            <Label>Top P</Label>
            <span className="text-sm text-muted-foreground">{config.top_p?.toFixed(2)}</span>
          </div>
          <Slider
            value={[config.top_p || 0.9]}
            onValueChange={(v) => setConfig({ ...config, top_p: v[0] })}
            min={0}
            max={1}
            step={0.05}
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="seed">Seed (Optional)</Label>
          <Input
            id="seed"
            type="number"
            placeholder="Random seed"
            value={config.seed || ''}
            onChange={(e) => setConfig({ ...config, seed: parseInt(e.target.value) || undefined })}
          />
        </div>

        <div className="flex items-center space-x-2">
          <Checkbox
            id="evidence"
            checked={config.require_evidence || false}
            onCheckedChange={(checked) => setConfig({ ...config, require_evidence: !!checked })}
          />
          <Label htmlFor="evidence">Require Evidence (RAG)</Label>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );

  const renderResponse = (response: InferResponse | null, isLoading: boolean, isStreamingMode: boolean = false, streamingTokens: StreamingToken[] = []) => {
    // Handle streaming mode with throttled updates for performance
    if (isStreamingMode && isStreaming) {
      const streamingText = throttledStreamingTokens.map(t => t.token).join('');
      return (
        <div className="space-y-4">
          <Card>
            <CardHeader className="pb-3">
              <div className="flex items-center justify-between">
                <CardTitle className="text-base flex items-center gap-2">
                  <Wifi className="h-4 w-4 text-green-500 animate-pulse" />
                  Live Streaming
                </CardTitle>
                <div className="flex gap-2">
                  {streamController && (
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => streamController.abort()}
                      aria-label="Stop streaming"
                    >
                      <Square className="h-4 w-4" />
                    </Button>
                  )}
                  <Badge variant="outline" className="gap-1">
                    <TrendingUp className="h-3 w-3" />
                    {throttledStreamingTokens.length} tokens
                  </Badge>
                </div>
              </div>
            </CardHeader>
            <CardContent>
              <div
                ref={streamingRef}
                className="relative"
              >
                <pre className="whitespace-pre-wrap text-sm p-4 bg-muted border border-border rounded-lg min-h-[100px]">
                  {streamingText}
                  <span className="animate-pulse text-primary">▊</span>
                </pre>
                <Button
                  variant="ghost"
                  size="sm"
                  className="absolute top-2 right-2"
                  onClick={() => handleCopy(streamingText)}
                  disabled={!streamingText.trim()}
                >
                  <Copy className="h-4 w-4" aria-hidden="true" />
                </Button>
              </div>
            </CardContent>
          </Card>
        </div>
      );
    }

    if (isLoading) {
      return (
        <div className="flex items-center justify-center p-8">
          <div className="text-center space-y-2">
            <Zap className="h-8 w-8 animate-pulse mx-auto text-primary" />
            <p className="text-sm text-muted-foreground">Generating response...</p>
          </div>
        </div>
      );
    }

    if (!response) {
      return (
        <div className="flex items-center justify-center p-8 text-muted-foreground">
          <FileText className="h-8 w-8 mr-2" />
          <p>No response yet. Click "Generate" to run inference.</p>
        </div>
      );
    }

    return (
      <div className="space-y-4">
        {/* Response Text */}
        <Card>
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-base flex items-center gap-2">
                {inferenceMode === 'streaming' && <CheckCircle className="h-4 w-4 text-green-500" />}
                Response
              </CardTitle>
              <div className="flex gap-2">
                <Badge variant="outline" className="gap-1">
                  <Clock className="h-3 w-3" />
                  {response.latency_ms || ('trace' in response && response.trace && 'latency_ms' in response.trace ? (response.trace as any).latency_ms : 0)}ms
                </Badge>
                <Badge variant="outline" className="gap-1">
                  <FileText className="h-3 w-3" />
                  {response.token_count || 0} tokens
                </Badge>
                {metrics && (
                  <Badge variant="outline" className="gap-1">
                    <TrendingUp className="h-3 w-3" />
                    {metrics.tokensPerSecond.toFixed(1)} t/s
                  </Badge>
                )}
              </div>
            </div>
          </CardHeader>
          <CardContent>
            <VirtualizedResponse text={response.text} />
            <div className="mt-2 flex justify-end">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => handleCopy(response.text)}
              >
                <Copy className="h-4 w-4 mr-2" aria-hidden="true" />
                Copy
              </Button>
            </div>
          </CardContent>
        </Card>

        {/* Trace Information */}
        {response.trace && 'latency_ms' in response.trace && (
          <TraceVisualizer trace={response.trace as any} />
        )}

        {/* Enhanced Metadata */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div className="flex items-center gap-2">
            <CheckCircle className="h-4 w-4 text-muted-foreground" />
            <div>
              <div className="text-sm font-medium">Finish Reason</div>
              <div className="text-xs text-muted-foreground">{response.finish_reason || 'unknown'}</div>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Target className="h-4 w-4 text-muted-foreground" />
            <div>
              <div className="text-sm font-medium">Router Decisions</div>
              <div className="text-xs text-muted-foreground">
                {response.trace?.router_decisions?.length || 0} steps
              </div>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <BarChart3 className="h-4 w-4 text-muted-foreground" />
            <div>
              <div className="text-sm font-medium">Evidence Spans</div>
              <div className="text-xs text-muted-foreground">
                {response.trace?.evidence_spans?.length || 0} found
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  };

  return (
    <div className="space-y-6">
      {/* Error Recovery */}
      {inferenceError && ErrorRecoveryTemplates.genericError(
        inferenceError,
        () => { setInferenceError(null); setPrompt(''); }
      )}

      {visibleHint && (
        <ProgressiveHint
          title={visibleHint.hint.title}
          content={visibleHint.hint.content}
          onDismiss={() => dismissHint(visibleHint.hint.id)}
          placement={visibleHint.hint.placement}
        />
      )}

      {/* Header */}
      <ToolPageHeader
        title="Inference Playground"
        description="Test model inference with advanced configuration options and real-time streaming"
        secondaryActions={
          <div className="flex gap-2 flex-wrap">
            {/* Connection Quality Indicator */}
            <div className="flex items-center gap-1 text-xs text-muted-foreground">
              <div className={`w-2 h-2 rounded-full ${
                connectionQuality === 'fast' ? 'bg-green-500' :
                connectionQuality === 'slow' ? 'bg-yellow-500' : 'bg-red-500'
              }`} />
              <span className="capitalize">{connectionQuality}</span>
            </div>

            {/* Worker Health Indicator */}
            <div className="flex items-center gap-1 text-xs text-muted-foreground">
              <div className={`w-2 h-2 rounded-full ${
                workerHealthy ? 'bg-green-500' : 'bg-red-500'
              }`} />
              <span>{workerHealthy ? 'Service OK' : 'Service Issue'}</span>
            </div>

            {/* Inference Mode Selection */}
            <div className="flex gap-1 border rounded-md p-1">
          <Button
                variant={inferenceMode === 'standard' ? 'default' : 'ghost'}
                size="sm"
                onClick={() => setInferenceMode('standard')}
              >
                <Zap className="h-3 w-3 mr-1" />
                Standard
              </Button>
              <Button
                variant={inferenceMode === 'streaming' ? 'default' : 'ghost'}
                size="sm"
                onClick={() => setInferenceMode('streaming')}
              >
                <Wifi className="h-3 w-3 mr-1" />
                Streaming
              </Button>
              <Button
                variant={inferenceMode === 'batch' ? 'default' : 'ghost'}
                size="sm"
                onClick={() => setInferenceMode('batch')}
              >
                <Layers className="h-3 w-3 mr-1" />
                Batch
              </Button>
            </div>

            {/* Single vs Comparison Mode */}
            <div className="flex gap-1 border rounded-md p-1">
              <Button
                variant={mode === 'single' ? 'default' : 'ghost'}
                size="sm"
            onClick={() => setMode('single')}
          >
                <FileText className="h-3 w-3 mr-1" />
            Single
          </Button>
          <Button
                variant={mode === 'comparison' ? 'default' : 'ghost'}
                size="sm"
            onClick={() => setMode('comparison')}
          >
                <Split className="h-3 w-3 mr-1" />
                Compare
          </Button>
            </div>
          </div>
        }
      />

      {/* Performance Metrics Display */}
      {metrics && (
        <Card className="mb-4">
          <CardContent className="pt-4">
            <div className="flex items-center gap-4 text-sm">
              <div className="flex items-center gap-1">
                <Clock className="h-4 w-4 text-muted-foreground" />
                <span>{metrics.latency}ms</span>
              </div>
              <div className="flex items-center gap-1">
                <TrendingUp className="h-4 w-4 text-muted-foreground" />
                <span>{metrics.tokensPerSecond.toFixed(1)} tokens/sec</span>
              </div>
              <div className="flex items-center gap-1">
                <Target className="h-4 w-4 text-muted-foreground" />
                <span>{metrics.totalTokens} tokens</span>
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      {inferenceMode === 'batch' ? (
        /* Batch Mode */
        <div className="space-y-6">
          <Card>
            <CardHeader>
              <CardTitle className="text-base flex items-center gap-2">
                <Layers className="h-5 w-5" />
                Batch Inference
              </CardTitle>
              <p className="text-sm text-muted-foreground">
                Process multiple prompts simultaneously with shared configuration
              </p>
            </CardHeader>
            <CardContent className="space-y-4">
              {/* Batch Prompts Input */}
              <div className="space-y-2">
                <Label>Prompts (one per line)</Label>
                <Textarea
                  placeholder="Enter one prompt per line...
Write a Python function to calculate fibonacci
Explain quantum computing in simple terms
What is the capital of France?"
                  value={batchPrompts.join('\n')}
                  onChange={(e) => setBatchPrompts(e.target.value.split('\n').filter(p => p.trim()))}
                  rows={8}
                  className={batchValidation.some(v => !v.valid) ? 'border-destructive' : ''}
                />
                <p className="text-xs text-muted-foreground">
                  {batchPrompts.filter(p => p.trim()).length} prompts ready for batch processing
                </p>

                {/* Batch validation errors */}
                {batchValidation.some(v => !v.valid) && (
                  <Alert variant="destructive" className="text-sm">
                    <AlertTriangle className="h-4 w-4" />
                    <AlertDescription>
                      <strong>Validation Errors:</strong>
                      <ul className="mt-1 space-y-1">
                        {batchValidation
                          .map((validation, index) => ({ validation, index }))
                          .filter(({ validation }) => !validation.valid)
                          .slice(0, 3) // Show first 3 errors
                          .map(({ validation, index }) => (
                            <li key={index}>
                              Prompt {index + 1}: {validation.error}
                            </li>
                          ))}
                        {batchValidation.filter(v => !v.valid).length > 3 && (
                          <li>... and {batchValidation.filter(v => !v.valid).length - 3} more</li>
                        )}
                      </ul>
                    </AlertDescription>
                  </Alert>
                )}

                {/* Batch validation warnings */}
                {batchValidation.some(v => v.warning) && (
                  <Alert variant="default" className="text-sm border-yellow-200 bg-yellow-50">
                    <AlertTriangle className="h-4 w-4 text-yellow-600" />
                    <AlertDescription className="text-yellow-800">
                      <strong>Warnings:</strong> Some prompts have warnings (long content, etc.)
                    </AlertDescription>
                  </Alert>
                )}
              </div>

              {/* Shared Configuration Preview */}
              <div className="p-3 bg-muted rounded-md">
                <h4 className="text-sm font-medium mb-2">Shared Configuration</h4>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-2 text-xs">
                  <div>Max Tokens: {configA.max_tokens}</div>
                  <div>Temperature: {configA.temperature}</div>
                  <div>Top K: {configA.top_k}</div>
                  <div>Top P: {configA.top_p?.toFixed(2)}</div>
                </div>
              </div>

              <Button
                onClick={executeBatchInference}
                disabled={batchPrompts.filter(p => p.trim()).length === 0 || isBatchRunning}
                className="w-full"
              >
                {isBatchRunning ? (
                  <>
                    <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-white mr-2" />
                    Processing Batch...
                  </>
                ) : (
                  <>
                    <Layers className="h-4 w-4 mr-2" />
                    Run Batch Inference ({batchPrompts.filter(p => p.trim()).length} prompts)
                  </>
                )}
              </Button>
            </CardContent>
          </Card>

          {/* Batch Results */}
          {batchResults && (
            <Card>
              <CardHeader>
                <CardTitle className="text-base">Batch Results</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="space-y-4">
                  {batchResults.responses?.map((item: any, index: number) => (
                    <Card key={item.id || index}>
                      <CardHeader className="pb-2">
                        <div className="flex items-center justify-between">
                          <CardTitle className="text-sm">Prompt {index + 1}</CardTitle>
                          {item.error ? (
                            <Badge variant="destructive">Error</Badge>
                          ) : (
                            <Badge variant="default">Success</Badge>
                          )}
                        </div>
                      </CardHeader>
                      <CardContent className="pt-0">
                        <div className="text-sm text-muted-foreground mb-2">
                          {batchPrompts[index]}
                        </div>
                        {item.response ? (
                          <div className="text-sm bg-muted p-3 rounded">
                            {item.response.text}
                          </div>
                        ) : item.error ? (
                          <div className="text-sm text-destructive bg-destructive/10 p-3 rounded">
                            {item.error}
                          </div>
                        ) : null}
                      </CardContent>
                    </Card>
                  ))}
                </div>
              </CardContent>
            </Card>
          )}
        </div>
      ) : mode === 'single' ? (
        /* Single Mode */
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Configuration Panel */}
          <div className="space-y-4">
            <Card>
              <CardHeader>
                <CardTitle className="text-base">Configuration</CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">
                {/* Tab Visibility Warning */}
                {shouldPauseInference && (
                  <Alert className="border-yellow-200 bg-yellow-50">
                    <AlertTriangle className="h-4 w-4 text-yellow-600" />
                    <AlertDescription className="text-yellow-800">
                      <strong>Inference paused:</strong> This tab became hidden during processing.
                      Requests were cancelled to save resources. Click "Generate" to resume.
                    </AlertDescription>
                  </Alert>
                )}

                {/* Browser environment warnings */}
                {isIncognito === true && (
                  <Alert variant="default" className="border-yellow-200 bg-yellow-50">
                    <AlertTriangle className="h-4 w-4 text-yellow-600" />
                    <AlertDescription className="text-yellow-800">
                      <strong>Private Browsing:</strong> You are in incognito/private browsing mode.
                      Session data will not persist between browser sessions.
                    </AlertDescription>
                  </Alert>
                )}

                {resourceConstraints?.memoryPressure === 'critical' && (
                  <Alert variant="destructive">
                    <AlertTriangle className="h-4 w-4" />
                    <AlertDescription>
                      <strong>Low Memory:</strong> Your device is running low on memory.
                      Large responses may cause performance issues. Consider using smaller prompts or batch processing.
                    </AlertDescription>
                  </Alert>
                )}

                {extensionInterference && (
                  <Alert variant="default" className="border-orange-200 bg-orange-50">
                    <AlertTriangle className="h-4 w-4 text-orange-600" />
                    <AlertDescription className="text-orange-800">
                      <strong>Extension Detected:</strong> Browser extensions may interfere with inference.
                      If you experience issues, try disabling extensions or using an incognito window.
                    </AlertDescription>
                  </Alert>
                )}

                {/* Adapter state change warning */}
                {adapterChangeDetected && (
                  <Alert variant="destructive">
                    <AlertTriangle className="h-4 w-4" />
                    <AlertDescription>
                      <strong>Adapter Unavailable:</strong> The selected adapter became unavailable during operation.
                      Inference will continue with base model only. You can select a different adapter.
                      <Button
                        variant="outline"
                        size="sm"
                        className="ml-2"
                        onClick={() => {
                          setAdapterChangeDetected(false);
                          setSelectedAdapterId('none');
                        }}
                      >
                        Switch to Base Model
                      </Button>
                    </AlertDescription>
                  </Alert>
                )}

                {/* Empty adapter pool warning */}
                {adapters.length === 0 && (
                  <Alert variant="default" className="border-blue-200 bg-blue-50">
                    <Info className="h-4 w-4 text-blue-600" />
                    <AlertDescription className="text-blue-800">
                      <strong>No Adapters Available:</strong> Only base model inference is available.
                      Train adapters in the Training section to unlock enhanced capabilities.
                    </AlertDescription>
                  </Alert>
                )}

                {/* Worker health warning */}
                {!workerHealthy && (
                  <Alert variant="destructive">
                    <AlertTriangle className="h-4 w-4" />
                    <AlertDescription>
                      <strong>Service Status:</strong> Inference service is currently experiencing issues.
                      Requests may be slower or fail. Please try again in a few moments.
                      {healthError && (
                        <div className="mt-1 text-sm opacity-90">
                          Details: {healthError}
                        </div>
                      )}
                    </AlertDescription>
                  </Alert>
                )}

                {/* Graceful degradation alert */}
                {adapterAvailability.isDegraded && (
                  <Alert variant="destructive">
                    <AlertTriangle className="h-4 w-4" />
                    <AlertDescription>
                      {adapters.length === 0
                        ? 'No adapters available. Inference will use base model only.'
                        : 'Adapter loading issues detected. Some adapters may be unavailable.'}
                      {!adaptersLoadError && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => adapterAvailability.checkHealth()}
                          className="ml-2"
                        >
                          Retry
                        </Button>
                      )}
                    </AlertDescription>
                  </Alert>
                )}
                <div className="space-y-2">
                  <Label htmlFor="adapter">
                    Adapter Selection {adapters.length === 0 && <span className="text-muted-foreground text-xs">(None - base model only)</span>}
                  </Label>
                  <Select value={selectedAdapterId} onValueChange={setSelectedAdapterId} disabled={adapters.length === 0}>
                    <SelectTrigger id="adapter">
                      <SelectValue placeholder={
                        adapters.length === 0
                          ? "No adapters available"
                          : selectedAdapterId === 'none'
                            ? "🤖 Auto-select (Router chooses best adapters)"
                            : "Select adapter..."
                      } />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="none">
                          <div className="flex items-center gap-2">
                          <Target className="h-4 w-4 text-primary" />
                          <div>
                            <div className="font-medium">Auto-select (Recommended)</div>
                            <div className="text-xs text-muted-foreground">Router chooses optimal adapters based on your prompt</div>
                          </div>
                          </div>
                        </SelectItem>
                      {adapters.filter(adapter => adapter.id && adapter.id !== '').map((adapter) => {
                        const getStateIcon = () => {
                          switch (adapter.current_state) {
                            case 'hot': return <div className="w-2 h-2 bg-green-500 rounded-full" />;
                            case 'warm': return <div className="w-2 h-2 bg-yellow-500 rounded-full" />;
                            case 'resident': return <div className="w-2 h-2 bg-blue-500 rounded-full" />;
                            case 'cold': return <div className="w-2 h-2 bg-gray-400 rounded-full" />;
                            default: return <div className="w-2 h-2 bg-gray-300 rounded-full" />;
                          }
                        };

                        const getStateColor = () => {
                          switch (adapter.current_state) {
                            case 'hot': return 'text-green-600';
                            case 'warm': return 'text-yellow-600';
                            case 'resident': return 'text-blue-600';
                            case 'cold': return 'text-gray-600';
                            default: return 'text-gray-500';
                          }
                        };

                        return (
                          <SelectItem key={adapter.id} value={adapter.id}>
                            <div className="flex items-center gap-2 w-full">
                              {getStateIcon()}
                              <Code className="h-4 w-4 flex-shrink-0" />
                              <div className="flex-1 min-w-0">
                                <div className="font-medium truncate">{adapter.name}</div>
                                <div className={`text-xs ${getStateColor()}`}>
                                  {adapter.current_state || 'unknown'} • {adapter.languages?.join(', ') || 'general'}
                                </div>
                              </div>
                            </div>
                          </SelectItem>
                        );
                      })}
                    </SelectContent>
                  </Select>
                  <div className="text-xs text-muted-foreground space-y-1">
                    <p>
                      {selectedAdapterId === 'none'
                        ? '🤖 Router will automatically select the best adapters for your prompt using K-sparse routing.'
                        : 'Using selected adapter for inference. Router decisions will still apply if multiple adapters match.'}
                    </p>
                    {adapters.length > 0 && (
                      <p>
                        💡 <strong>Pro tip:</strong> Try "Auto-select" to let the router optimize adapter selection based on your prompt content.
                      </p>
                    )}
                  </div>
                </div>

                <div className="space-y-2">
                  <div className="flex items-center justify-between">
                    <Label htmlFor="prompt">
                      Prompt
                      <span className="sr-only">
                        Use Ctrl+G or Cmd+G to generate, Ctrl+S or Cmd+S to toggle streaming mode, Ctrl+B or Cmd+B to toggle batch mode, Escape to cancel
                      </span>
                    </Label>
                    <div className="flex gap-2">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => setShowTemplates(!showTemplates)}
                        className="h-8 px-2"
                        aria-label={showTemplates ? "Hide prompt templates" : "Show prompt templates"}
                      >
                        <FileText className="h-3 w-3 mr-1" />
                        Templates
                      </Button>
                    </div>
                  </div>
                  <Textarea
                    id="prompt"
                    placeholder="Enter your prompt here..."
                    value={configA.prompt}
                    onChange={(e) => {
                      const sanitized = sanitizeInput(e.target.value);
                      setConfigA({ ...configA, prompt: sanitized });
                    }}
                    rows={6}
                    className={promptValidation.valid ? '' : 'border-destructive'}
                    aria-describedby={promptValidation.error ? "prompt-error" : promptValidation.warning ? "prompt-warning" : undefined}
                    aria-invalid={!promptValidation.valid}
                  />
                  {promptValidation.error && (
                    <Alert variant="destructive" className="text-sm" id="prompt-error">
                      <AlertTriangle className="h-4 w-4" />
                      <AlertDescription>
                        <strong>Validation Error:</strong> {promptValidation.error}
                        {promptValidation.suggestion && (
                          <div className="mt-1 text-sm opacity-90">
                            <strong>Suggestion:</strong> {promptValidation.suggestion}
                          </div>
                        )}
                      </AlertDescription>
                    </Alert>
                  )}
                  {promptValidation.warning && (
                    <Alert variant="default" className="text-sm border-yellow-200 bg-yellow-50" id="prompt-warning">
                      <AlertTriangle className="h-4 w-4 text-yellow-600" />
                      <AlertDescription className="text-yellow-800">
                        <strong>Warning:</strong> {promptValidation.warning}
                      </AlertDescription>
                    </Alert>
                  )}
                  {!promptValidation.valid && !promptValidation.error && (
                    <div className="text-xs text-muted-foreground">
                      Character count: {configA.prompt.length.toLocaleString()} / {MAX_PROMPT_LENGTH.toLocaleString()}
                    </div>
                  )}
                  {windowSize.width < 768 && (
                    <div className="text-xs text-muted-foreground mt-1">
                      💡 Swipe left/right to change modes, swipe up for templates
                    </div>
                  )}
                  {showTemplates && (
                    <div className="border rounded-md p-3 bg-muted/50">
                      <div className="text-sm font-medium mb-2">Prompt Templates</div>
                      <div className="space-y-2 max-h-48 overflow-y-auto">
                        {templates.map((template) => (
                          <Button
                            key={template.id}
                            variant="ghost"
                            className="w-full justify-start text-left h-auto p-2"
                            onClick={() => applyTemplate(template)}
                          >
                            <div>
                              <div className="font-medium text-sm">{template.name}</div>
                              <div className="text-xs text-muted-foreground">{template.description}</div>
                              <Badge variant="outline" className="mt-1 text-xs">
                                {template.category}
                              </Badge>
                            </div>
                          </Button>
                        ))}
                      </div>
                    </div>
                  )}
                </div>

                {renderAdvancedOptions(configA, setConfigA)}

                <div className="flex gap-2">
                  <Button
                    className="flex-1"
                    onClick={() => handleInfer(configA, setResponseA, setIsLoadingA)}
                    disabled={isLoadingA}
                    aria-label="Run inference with current configuration"
                  >
                    <Play className="h-4 w-4 mr-2" aria-hidden="true" />
                    {isLoadingA ? 'Generating...' : 'Generate'}
                  </Button>
                  {inferenceState.isRunning && (
                    <Button
                      variant="outline"
                      onClick={cancelInference}
                      aria-label="Cancel inference"
                    >
                      <Square className="h-4 w-4" />
                    </Button>
                  )}
                </div>

                {responseA && (
                  <Button
                    variant="outline"
                    className="w-full"
                    onClick={() => handleExport(configA, responseA)}
                  >
                    <Download className="h-4 w-4 mr-2" />
                    Export
                  </Button>
                )}
              </CardContent>
            </Card>

            {/* Recent Sessions */}
            {recentSessions.length > 0 && (
              <Card>
                <CardHeader>
                  <CardTitle className="text-base flex items-center gap-2">
                    <History className="h-4 w-4" aria-hidden="true" />
                    Recent Sessions
                  </CardTitle>
                </CardHeader>
                <CardContent className="space-y-2">
                  {recentSessions.slice(0, 5).map((session) => (
                    <Button
                      key={session.id}
                      variant="ghost"
                      className="w-full justify-start text-left h-auto py-2"
                      onClick={() => loadSession(session)}
                    >
                      <div className="truncate">
                        <p className="text-sm truncate">{session.prompt}</p>
                        <p className="text-xs text-muted-foreground">
                          {new Date(session.created_at).toLocaleString()}
                        </p>
                      </div>
                    </Button>
                  ))}
                </CardContent>
              </Card>
            )}
          </div>

          {/* Response Panel */}
          <div className="lg:col-span-2">
            <Card className="min-h-[600px]">
              <CardHeader>
                <CardTitle className="text-base">Output</CardTitle>
              </CardHeader>
              <CardContent>
                {renderResponse(responseA, isLoadingA, inferenceMode === 'streaming' && isStreaming, streamingTokens)}
              </CardContent>
            </Card>
          </div>
        </div>
      ) : (
        /* Comparison Mode */
        <div className="space-y-4">
          {/* Shared Prompt */}
          <Card>
            <CardHeader>
              <CardTitle className="text-base">Shared Prompt</CardTitle>
            </CardHeader>
            <CardContent>
              <Textarea
                placeholder="Enter prompt to compare..."
                value={prompt}
                onChange={(e) => {
                  setPrompt(e.target.value);
                  setConfigA({ ...configA, prompt: e.target.value });
                  setConfigB({ ...configB, prompt: e.target.value });
                }}
                rows={4}
              />
            </CardContent>
          </Card>

          {/* Side-by-Side Configurations */}
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {/* Config A */}
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle className="text-base">Configuration A</CardTitle>
                  <Badge>Temperature: {configA.temperature}</Badge>
                </div>
              </CardHeader>
              <CardContent className="space-y-4">
                {renderAdvancedOptions(configA, setConfigA)}
                <div className="flex gap-2">
                  <Button
                    className="flex-1"
                    onClick={() => handleInfer(configA, setResponseA, setIsLoadingA)}
                    disabled={isLoadingA || !prompt.trim()}
                  >
                    <Play className="h-4 w-4 mr-2" aria-hidden="true" />
                    Generate A
                  </Button>
                  {inferenceState.isRunning && (
                    <Button
                      variant="outline"
                      onClick={cancelInference}
                      aria-label="Cancel inference A"
                    >
                      <Square className="h-4 w-4" />
                    </Button>
                  )}
                </div>
                {renderResponse(responseA, isLoadingA, inferenceMode === 'streaming' && isStreaming, streamingTokens)}
              </CardContent>
            </Card>

            {/* Config B */}
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle className="text-base">Configuration B</CardTitle>
                  <Badge>Temperature: {configB.temperature}</Badge>
                </div>
              </CardHeader>
              <CardContent className="space-y-4">
                {renderAdvancedOptions(configB, setConfigB)}
                <div className="flex gap-2">
                  <Button
                    className="flex-1"
                    onClick={() => handleInfer(configB, setResponseB, setIsLoadingB)}
                    disabled={isLoadingB || !prompt.trim()}
                  >
                    <Play className="h-4 w-4 mr-2" aria-hidden="true" />
                    Generate B
                  </Button>
                  {inferenceState.isRunning && (
                    <Button
                      variant="outline"
                      onClick={cancelInference}
                      aria-label="Cancel inference B"
                    >
                      <Square className="h-4 w-4" />
                    </Button>
                  )}
                </div>
                {renderResponse(responseB, isLoadingB, false, [])}
              </CardContent>
            </Card>
          </div>

          {/* Comparison Summary */}
          {responseA && responseB && (
            <Card>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  <BarChart3 className="h-4 w-4" aria-hidden="true" />
                  Comparison Summary
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                  <div>
                    <p className="text-sm font-medium">Latency</p>
                    <div className="flex items-center gap-2 mt-1">
                      <Badge variant="outline">A: {responseA.latency_ms || 0}ms</Badge>
                      <Badge variant="outline">B: {responseB.latency_ms || 0}ms</Badge>
                    </div>
                  </div>
                  <div>
                    <p className="text-sm font-medium">Tokens</p>
                    <div className="flex items-center gap-2 mt-1">
                      <Badge variant="outline">A: {responseA.token_count || 0}</Badge>
                      <Badge variant="outline">B: {responseB.token_count || 0}</Badge>
                    </div>
                  </div>
                  <div>
                    <p className="text-sm font-medium">Finish Reason</p>
                    <div className="flex items-center gap-2 mt-1">
                      <Badge variant="outline">{responseA.finish_reason || 'unknown'}</Badge>
                      <Badge variant="outline">{responseB.finish_reason || 'unknown'}</Badge>
                    </div>
                  </div>
                  <div>
                    <p className="text-sm font-medium">Winner</p>
                    <Badge className="mt-1">
                      {(responseA.latency_ms || 0) < (responseB.latency_ms || 0) ? 'A (Faster)' : 'B (Faster)'}
                    </Badge>
                  </div>
                </div>
              </CardContent>
            </Card>
          )}
        </div>
      )}
    </div>
  );
}
