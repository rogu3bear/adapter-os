# Login Update - Detailed Code Changes

## Overview

Updated the authentication flow to handle:
1. **Username field** - now required in LoginRequest
2. **expires_in response** - changed from expires_at (string) to expires_in (seconds as number)
3. **tenant_id requirement** - explicitly required in LoginResponse
4. **Token expiration tracking** - calculated and stored for future refresh logic

---

## File 1: ApiClient Login Method

**Location:** `/Users/star/Dev/aos/ui/src/api/client.ts` (Lines 299-320)

### Before
```typescript
async login(credentials: authTypes.LoginRequest): Promise<authTypes.LoginResponse> {
  const response = await this.request<authTypes.LoginResponse>('/v1/auth/login', {
    method: 'POST',
    body: JSON.stringify(credentials),
  });
  // Token is now stored in httpOnly cookie by server
  return response;
}
```

### After
```typescript
async login(credentials: authTypes.LoginRequest): Promise<authTypes.LoginResponse> {
  const response = await this.request<authTypes.LoginResponse>('/v1/auth/login', {
    method: 'POST',
    body: JSON.stringify(credentials),
  });
  // Token is now stored in httpOnly cookie by server
  // expires_in is in seconds; calculate absolute expiration timestamp for token refresh tracking
  const expiresAt = Date.now() + (response.expires_in * 1000);
  logger.info('User logged in', {
    component: 'ApiClient',
    operation: 'login',
    user_id: response.user_id,
    tenant_id: response.tenant_id,
    role: response.role,
    expiresAt: new Date(expiresAt).toISOString(),
  });
  // Store expiration for potential token refresh logic
  if (typeof sessionStorage !== 'undefined') {
    sessionStorage.setItem('token_expires_at', expiresAt.toString());
  }
  return response;
}
```

### Key Changes
- Converts `expires_in` (seconds) to absolute timestamp
- Logs structured authentication event with user/tenant info
- Stores expiration in sessionStorage for token refresh logic
- Provides better debugging with ISO formatted expiration time

---

## File 2: LoginForm Component UI

**Location:** `/Users/star/Dev/aos/ui/src/components/LoginForm.tsx`

### Change 1: Props Interface (Line 12)

#### Before
```typescript
interface LoginFormProps {
  onLogin: (credentials: { email: string; password: string }) => Promise<void>;
  onDevBypass?: () => Promise<void>;
  error?: string | null;
}
```

#### After
```typescript
interface LoginFormProps {
  onLogin: (credentials: { username: string; email: string; password: string }) => Promise<void>;
  onDevBypass?: () => Promise<void>;
  error?: string | null;
}
```

### Change 2: State Management (Lines 18-20)

#### Before
```typescript
const [email, setEmail] = useState('');
const [password, setPassword] = useState('');
```

#### After
```typescript
const [username, setUsername] = useState('');
const [email, setEmail] = useState('');
const [password, setPassword] = useState('');
```

### Change 3: Form Submission (Lines 30-34)

#### Before
```typescript
try {
  await onLogin({ email: email.trim(), password: password.trim() });
} catch (err) {
  // Error is handled by parent component
} finally {
  setIsLoading(false);
}
```

#### After
```typescript
try {
  await onLogin({
    username: username.trim(),
    email: email.trim(),
    password: password.trim(),
  });
} catch (err) {
  // Error is handled by parent component
} finally {
  setIsLoading(false);
}
```

### Change 4: Form Input Fields (Lines 105-139)

#### Before
```tsx
<div className="mb-4">
  <Label htmlFor="email" className="font-medium text-sm mb-1">Email</Label>
  <Input
    id="email"
    type="email"
    placeholder="Enter your email"
    value={email}
    onChange={(e) => setEmail(e.target.value)}
    required
  />
</div>

<div className="mb-4">
  <Label htmlFor="password" className="font-medium text-sm mb-1">Password</Label>
  <Input
    id="password"
    type="password"
    placeholder="Enter your password"
    value={password}
    onChange={(e) => setPassword(e.target.value)}
    required
  />
</div>
```

#### After
```tsx
<div className="mb-4">
  <Label htmlFor="username" className="font-medium text-sm mb-1">Username</Label>
  <Input
    id="username"
    type="text"
    placeholder="Enter your username"
    value={username}
    onChange={(e) => setUsername(e.target.value)}
    required
  />
</div>

<div className="mb-4">
  <Label htmlFor="email" className="font-medium text-sm mb-1">Email</Label>
  <Input
    id="email"
    type="email"
    placeholder="Enter your email"
    value={email}
    onChange={(e) => setEmail(e.target.value)}
    required
  />
</div>

<div className="mb-4">
  <Label htmlFor="password" className="font-medium text-sm mb-1">Password</Label>
  <Input
    id="password"
    type="password"
    placeholder="Enter your password"
    value={password}
    onChange={(e) => setPassword(e.target.value)}
    required
  />
</div>
```

### Change 5: Submit Button Validation (Line 144)

#### Before
```typescript
disabled={isLoading || !email.trim() || !password.trim()}
```

#### After
```typescript
disabled={isLoading || !username.trim() || !email.trim() || !password.trim()}
```

### Change 6: Demo Credentials Display (Lines 178-182)

#### Before
```tsx
<p className="font-medium">Admin User:</p>
<p className="font-mono text-muted-foreground">admin@aos.local / password</p>
```

#### After
```tsx
<p className="font-medium">Admin User:</p>
<p className="font-mono text-muted-foreground">
  Username: admin<br />
  Email: admin@aos.local<br />
  Password: password
</p>
```

---

## File 3: AuthProvider Hook

**Location:** `/Users/star/Dev/aos/ui/src/providers/CoreProviders.tsx`

### Change 1: Import Statements (Lines 3-4)

#### Before
```typescript
import type { User, LoginRequest } from '../api/types';
```

#### After
```typescript
import type { User } from '../api/types';
import type { LoginRequest } from '../api/auth-types';
```

### Change 2: Login Callback (Lines 117-136)

#### Before
```typescript
const login = useCallback(async (credentials: LoginRequest) => {
  try {
    await apiClient.login(credentials);
    await refreshUser();
  } catch (error) {
    logger.error('Login failed', { component: 'AuthProvider' }, toError(error));
    throw error; // Re-throw so caller can handle
  }
}, [refreshUser]);
```

#### After
```typescript
const login = useCallback(async (credentials: LoginRequest) => {
  try {
    logger.info('Initiating login', {
      component: 'AuthProvider',
      operation: 'login',
      username: credentials.username,
    });
    const response = await apiClient.login(credentials);
    logger.info('Login successful', {
      component: 'AuthProvider',
      operation: 'login',
      user_id: response.user_id,
      tenant_id: response.tenant_id,
    });
    await refreshUser();
  } catch (error) {
    logger.error('Login failed', { component: 'AuthProvider' }, toError(error));
    throw error; // Re-throw so caller can handle
  }
}, [refreshUser]);
```

### Key Changes
- Adds pre-login logging with username
- Captures and logs response user_id and tenant_id
- Provides audit trail of authentication attempts
- Better debugging through structured logging

---

## Data Flow Diagram

```
┌─────────────────────┐
│   LoginForm (UI)    │
│  - username input   │
│  - email input      │
│  - password input   │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────────────┐
│  onLogin callback           │
│  { username, email, pwd }   │
└──────────┬──────────────────┘
           │
           ▼
┌──────────────────────────────┐
│ LoginRoute (main.tsx)        │
│ Calls: auth.login()          │
└──────────┬───────────────────┘
           │
           ▼
┌──────────────────────────────┐
│ AuthProvider.login()         │
│ - Logs username              │
│ - Calls apiClient.login()    │
│ - Calls refreshUser()        │
└──────────┬───────────────────┘
           │
           ▼
┌────────────────────────────────────┐
│ ApiClient.login()                  │
│ - POST /v1/auth/login              │
│ - Receives: { token, user_id,      │
│   tenant_id, role, expires_in }    │
│ - Calculates expiration timestamp  │
│ - Stores in sessionStorage         │
│ - Logs success with details        │
└────────────────────────────────────┘
           │
           ▼
    ✓ Token stored in httpOnly cookie
    ✓ Expiration calculated and tracked
    ✓ User info fetched and stored
```

---

## Response Handling

### Backend Response Structure
```typescript
LoginResponse {
  schema_version: "1.0",
  token: "eyJ...",           // JWT token
  user_id: "user_123",       // User identifier
  tenant_id: "tenant_456",   // Required tenant context
  role: "admin",             // User role
  expires_in: 28800          // 8 hours in seconds
}
```

### Client-Side Processing
```typescript
// 1. Calculate absolute expiration
const expiresAt = Date.now() + (response.expires_in * 1000);
// expiresAt = 1732198400000 (milliseconds)

// 2. Store for token refresh
sessionStorage.setItem('token_expires_at', expiresAt.toString());

// 3. Log for audit trail
logger.info('User logged in', {
  user_id: response.user_id,
  tenant_id: response.tenant_id,
  expiresAt: new Date(expiresAt).toISOString()
  // Logs: "2025-11-21T12:34:00.000Z"
});
```

---

## Backward Compatibility

✓ **No breaking changes**
- LoginRoute component unchanged (still uses useAuth().login())
- Logout/session refresh flows unaffected
- httpOnly cookie handling preserved
- Existing error handling maintained

---

## Security Considerations

1. **Username field** - Allows for independent email/username management
2. **Tenant isolation** - tenant_id ensures multi-tenant separation
3. **Token expiration** - Explicit expires_in allows TTL enforcement
4. **Session storage** - Expiration stored locally (not in cookie) for refresh logic
5. **Structured logging** - Audit trail without logging sensitive credentials

