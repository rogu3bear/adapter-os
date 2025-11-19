# UI TypeScript Errors - Quick Fix Guide

## Critical Fixes (5 min each)

### Fix 1: Remove Self-Import in alert.tsx
**File:** `/Users/star/Dev/aos/ui/src/components/ui/alert.tsx`
**Line:** 5
**Current Code:**
```typescript
import { Alert, AlertDescription, AlertTitle } from './alert';
```
**Action:** DELETE this entire line
**Reason:** File defines these components itself; cannot import from self
**Errors Fixed:** 3 (TS2440 × 3)

---

### Fix 2: Remove Duplicate Key in help-text.ts
**File:** `/Users/star/Dev/aos/ui/src/data/help-text.ts`
**Lines:** 20-22
**Current Code:**
```typescript
{
  id: 'adapters',
  title: 'Adapters',
  content: 'Manage adapters for specialized AI capabilities. Create, train, and deploy adapters for specific domains.',  // Line 20
  content: 'Manage LoRA adapters for specialized AI capabilities. Create, train, and deploy adapters for specific domains.',  // Line 22 - DUPLICATE
  category: 'navigation'
}
```
**Action:** Delete line 20 OR 22 (keep the updated version with "LoRA")
**New Code:**
```typescript
{
  id: 'adapters',
  title: 'Adapters',
  content: 'Manage LoRA adapters for specialized AI capabilities. Create, train, and deploy adapters for specific domains.',
  category: 'navigation'
}
```
**Errors Fixed:** 1 (TS1117 × 1)

---

### Fix 3: Add Import to Tenants.tsx
**File:** `/Users/star/Dev/aos/ui/src/components/Tenants.tsx`
**Line:** Add after existing imports
**Action:** Add this line at the top with other imports:
```typescript
import * as types from '../api/types';
```
**Context:** Line 409 uses `types.Tenant` but types not imported
**Errors Fixed:** 1 (TS2503 × 1)

---

### Fix 4: Install embla-carousel-react
**File:** `ui/` directory
**Command:**
```bash
cd /Users/star/Dev/aos/ui && pnpm install embla-carousel-react
```
**Errors Fixed:** 4 (TS2552 × 4)

---

### Fix 5: Add VariantProps Imports
**File 1:** `/Users/star/Dev/aos/ui/src/components/ui/alert.tsx`
**Line:** Add after line 2
```typescript
import { VariantProps } from 'class-variance-authority';
```

**File 2:** `/Users/star/Dev/aos/ui/src/components/ui/button.tsx`
**Line:** Add with other imports
```typescript
import { VariantProps } from 'class-variance-authority';
```

**Errors Fixed:** 2 (TS2304 × 2)

---

## High Priority Fixes (Error Handling)

### Fix 6: Error → String Assignments

**Pattern:** Components catch errors and assign to string state

#### 6a. AdaptersPage.tsx:112
**Current:**
```typescript
const [error, setError] = useState<string>('');

try {
  // ...
} catch (error) {
  setError(error);  // ← Error, should be string
}
```

**Fixed:**
```typescript
catch (error) {
  setError(error instanceof Error ? error.message : String(error));
}
```

#### 6b. BaseModelLoader.tsx (2 instances: lines 105, 116)
**Current:**
```typescript
catch (e) {
  setLoadError(e);
}
```

**Fixed:**
```typescript
catch (e) {
  setLoadError(e instanceof Error ? e.message : String(e));
}
```

#### 6c. TrainingMonitor.tsx (3 instances: lines 353, 365, 377)
**Current:**
```typescript
setError(err);
```

**Fixed:**
```typescript
setError(err instanceof Error ? err.message : String(err));
```

**Affected Files (same pattern):**
- ModelImportWizard.tsx:332
- LanguageBaseAdapterDialog.tsx:128
- PolicyEditor.tsx:316
- RouterConfigPage.tsx:256
- SingleFileAdapterTrainer.tsx:484
- SpawnWorkerModal.tsx:140
- TestingPage.tsx:51, 87
- TrainingPage.tsx:116

**Errors Fixed:** 11+ (TS2322, TS2345 combined)

---

### Fix 7: Fix logger.warn() Calls

**Pattern:** Called with 3 arguments instead of 2

#### 7a. useProgressOperation.ts:118
**Current:**
```typescript
logger.warn('Failed to store operation history', {
  component: 'useProgressOperation',
  operation: 'storeOperationData',
  type,
}, error);
```

**Fixed:**
```typescript
logger.warn('Failed to store operation history', {
  component: 'useProgressOperation',
  operation: 'storeOperationData',
  type,
  error,
});
```

#### Similar fixes needed at:
- useProgressOperation.ts:159
- useProgressOperation.ts:226
- useSSE.ts:103

**Errors Fixed:** 4 (TS2554 × 4)

---

## Type Definition Updates

### Fix 8: Extend Adapter Type

**File:** `/Users/star/Dev/aos/ui/src/api/types.ts`
**Location:** Adapter interface (around line 405)

**Add these properties:**
```typescript
export interface Adapter {
  id: string;
  adapter_id: string;
  name: string;
  hash_b3: string;
  rank: number;
  tier: number;
  languages_json?: string;
  framework?: string;

  // ADD THESE:
  languages?: string[];  // ← New
  state?: AdapterState;  // ← New

  // ... rest of existing properties
}
```

**Errors Fixed:** 5 (TS2339 × 5)

---

### Fix 9: Extend RoutingDecision Type

**File:** `/Users/star/Dev/aos/ui/src/api/types.ts`
**Location:** RoutingDecision interface (around line 881)

**Add property:**
```typescript
export interface RoutingDecision {
  id: string;
  tenant_id: string;
  timestamp: string;
  // ... existing properties ...

  // ADD THIS:
  adapters?: string[];  // ← New
}
```

**Errors Fixed:** 1 (TS2339 × 1)

---

### Fix 10: Extend RouteConfig Type

**File:** `/Users/star/Dev/aos/ui/src/utils/navigation.ts`
**Search for:** RouteConfig interface definition

**Add properties:**
```typescript
export interface RouteConfig {
  path: string;
  label: string;
  icon?: React.ComponentType<any>;

  // ADD THESE:
  disabled?: boolean;  // ← New
  external?: boolean;  // ← New
}
```

**Errors Fixed:** 2 (TS2339 × 2)

---

## API Method Additions

### Fix 11: Add get() Method to ApiClient

**File:** `/Users/star/Dev/aos/ui/src/api/client.ts`
**Location:** ApiClient class (add after line 300 or with other methods)

**Add method:**
```typescript
async get<T>(path: string): Promise<T> {
  return this.request<T>(path, {
    method: 'GET',
    headers: this.getDefaultHeaders()
  });
}

private getDefaultHeaders(): HeadersInit {
  const headers: HeadersInit = {
    'Content-Type': 'application/json',
  };
  if (this.token) {
    headers['Authorization'] = `Bearer ${this.token}`;
  }
  return headers;
}
```

**Called by:** PluginStatusWidget.tsx:20
**Errors Fixed:** 1 (TS2339 × 1)

---

### Fix 12: Implement waitForHealthy() Method

**File:** `/Users/star/Dev/aos/ui/src/services/ServiceLifecycleManager.ts`
**Location:** ServiceLifecycleManager class

**Add method:**
```typescript
async waitForHealthy(maxRetries: number = 30, delayMs: number = 1000): Promise<boolean> {
  for (let i = 0; i < maxRetries; i++) {
    const status = await this.getHealthStatus();
    if (status === 'healthy') {
      return true;
    }
    await new Promise(resolve => setTimeout(resolve, delayMs));
  }
  return false;
}

private async getHealthStatus(): Promise<'healthy' | 'unhealthy' | 'unknown'> {
  try {
    const response = await fetch('http://localhost:8080/healthz');
    if (response.ok) {
      return 'healthy';
    }
    return 'unhealthy';
  } catch {
    return 'unknown';
  }
}
```

**Called by:** Lines 178, 322
**Errors Fixed:** 2 (TS2339 × 2)

---

## React Ref Fixes

### Fix 13: Fix Boolean Ref in InferencePlayground

**File:** `/Users/star/Dev/aos/ui/src/components/InferencePlayground.tsx`
**Line:** 669

**Current:**
```typescript
const isScrolling = useRef<boolean>(false);
// ...
<div ref={isScrolling} className="...">
```

**Fixed:**
```typescript
const scrollContainerRef = useRef<HTMLDivElement>(null);
const isScrolling = useRef<boolean>(false);
// ...
<div ref={scrollContainerRef} className="...">
```

**Errors Fixed:** 1 (TS2322 × 1)

---

## Summary of Quick Fixes

| Priority | Fix | Time | Errors Fixed |
|----------|-----|------|--------------|
| Critical | Remove alert.tsx import | 1 min | 3 |
| Critical | Delete duplicate key | 1 min | 1 |
| Critical | Add Tenants import | 1 min | 1 |
| Critical | Install embla-carousel | 2 min | 4 |
| Critical | Add VariantProps imports | 2 min | 2 |
| High | Fix Error→String (11 files) | 30 min | 14 |
| High | Fix logger.warn() calls (4 files) | 10 min | 4 |
| High | Extend Adapter type | 5 min | 5 |
| High | Extend other types (3 types) | 10 min | 3 |
| High | Add API methods (2 methods) | 10 min | 3 |
| High | Fix React ref | 5 min | 1 |

**Total Time: ~80-90 minutes for all fixes**

---

## Testing After Fixes

After completing all fixes, run:

```bash
cd /Users/star/Dev/aos/ui

# Check TypeScript compilation
pnpm exec tsc --noEmit

# Expected output: No errors

# Build for production
pnpm build

# Run linter
pnpm exec eslint src/
```

Expected result: **0 TypeScript errors** across all files
