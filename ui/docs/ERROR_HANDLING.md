# Error Handling in AdapterOS UI

This document describes the error handling patterns and utilities available in the AdapterOS UI.

## Overview

The UI implements a comprehensive error handling strategy with multiple layers:

1. **Global Error Boundary** - Catches catastrophic application errors
2. **Page Error Boundaries** - Catches errors at the page level
3. **Section Error Boundaries** - Catches errors in specific sections/components
4. **Modal Error Boundaries** - Catches errors in modals and dialogs
5. **Query Error Handlers** - Handles API/data fetching errors
6. **Custom Error Hooks** - Reusable error handling logic

## Components

### GlobalErrorHandler

Top-level error boundary that wraps the entire application. Provides recovery options for catastrophic failures.

```tsx
import { GlobalErrorHandler } from '@/components/ui/global-error-handler';

<GlobalErrorHandler>
  <App />
</GlobalErrorHandler>
```

**Features:**
- Full-page error fallback UI
- Retry functionality
- Navigate to dashboard
- Stack traces in development mode
- Automatic error logging

### withPageErrorBoundary

Higher-order component (HOC) that wraps page components with error boundaries.

```tsx
import { withPageErrorBoundary } from '@/components/ui/with-page-error-boundary';

function MyPage() {
  // Page content
}

export default withPageErrorBoundary(MyPage, { pageName: 'My Page' });
```

**Features:**
- Page-level error isolation
- Custom error messages per page
- Retry, go back, and go home actions
- Automatic error logging with page context

### SectionErrorBoundary

Component-level error boundary for isolating errors in specific sections.

```tsx
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';

<SectionErrorBoundary sectionName="User Profile" severity="warning">
  <UserProfileSection />
</SectionErrorBoundary>
```

**Features:**
- Granular error isolation
- Customizable severity (warning/error)
- Section-specific error messages
- Retry functionality
- Rest of page continues working

### ModalErrorBoundary

Specialized error boundary for modals and dialogs.

```tsx
import { ModalErrorBoundary } from '@/components/ui/modal-error-boundary';

<Dialog>
  <ModalErrorBoundary onClose={() => setOpen(false)}>
    <DialogContent>
      {/* Modal content */}
    </DialogContent>
  </ModalErrorBoundary>
</Dialog>
```

**Features:**
- Modal-specific error UI
- Close modal on error option
- Retry functionality
- Prevents modal errors from breaking the page

## Utilities

### QueryWrapper

Declarative wrapper for TanStack Query results with automatic error/loading/empty state handling.

```tsx
import { QueryWrapper } from '@/components/ui/query-wrapper';

const adaptersQuery = useQuery({
  queryKey: ['adapters'],
  queryFn: fetchAdapters,
});

return (
  <QueryWrapper
    query={adaptersQuery}
    loadingMessage="Loading adapters..."
    errorTitle="Failed to load adapters"
    emptyMessage="No adapters found"
    isEmpty={(data) => data.length === 0}
  >
    {(data) => <AdaptersList adapters={data} />}
  </QueryWrapper>
);
```

**Features:**
- Automatic loading state
- Automatic error state with retry
- Empty state handling
- Type-safe data rendering

### MultiQueryWrapper

Wrapper for handling multiple TanStack Query results.

```tsx
import { MultiQueryWrapper } from '@/components/ui/query-wrapper';

const adaptersQuery = useQuery(['adapters'], fetchAdapters);
const modelsQuery = useQuery(['models'], fetchModels);

return (
  <MultiQueryWrapper
    queries={[
      { query: adaptersQuery, name: 'Adapters' },
      { query: modelsQuery, name: 'Models' },
    ]}
    loadingMessage="Loading data..."
  >
    {() => (
      <div>
        <AdaptersList adapters={adaptersQuery.data} />
        <ModelsList models={modelsQuery.data} />
      </div>
    )}
  </MultiQueryWrapper>
);
```

## Hooks

### useErrorHandler

Hook for consistent error handling with logging and toast notifications.

```tsx
import { useErrorHandler } from '@/hooks/ui/useErrorHandler';

const { handleError } = useErrorHandler({
  component: 'TrainingPage',
  operation: 'loadJobs',
  showToast: true,
  severity: 'error',
});

try {
  await fetchData();
} catch (error) {
  handleError(error);
}
```

**Features:**
- Automatic error message extraction
- Toast notifications with customizable severity
- Error logging with context
- TypeScript-safe error handling
- Request ID extraction from API errors

### useQueryErrorHandler

Specialized error handler for TanStack Query with convenient callback creators.

```tsx
import { useQueryErrorHandler } from '@/hooks/ui/useErrorHandler';

const { onError } = useQueryErrorHandler({
  component: 'AdaptersPage',
  showToast: true,
});

const { data } = useQuery({
  queryKey: ['adapters'],
  queryFn: fetchAdapters,
  onError: onError('loadAdapters'),
});
```

## Best Practices

### 1. Layer Error Boundaries

Use multiple error boundary layers for better isolation:

```tsx
// Page level
export default withPageErrorBoundary(TrainingPage, { pageName: 'Training' });

// Section level within page
function TrainingPage() {
  return (
    <PageWrapper>
      <SectionErrorBoundary sectionName="Training Jobs">
        <TrainingJobsTab />
      </SectionErrorBoundary>

      <SectionErrorBoundary sectionName="Datasets">
        <DatasetsTab />
      </SectionErrorBoundary>
    </PageWrapper>
  );
}
```

### 2. Use QueryWrapper for Declarative Data Fetching

Instead of manually handling loading/error states:

```tsx
// ❌ Manual handling
const { data, isLoading, error, refetch } = useQuery(...);

if (isLoading) return <LoadingState />;
if (error) return <ErrorMessage error={error} onRetry={refetch} />;
if (!data) return <EmptyState />;

return <DataDisplay data={data} />;

// ✅ Use QueryWrapper
return (
  <QueryWrapper
    query={useQuery(...)}
    loadingMessage="Loading..."
    emptyMessage="No data"
  >
    {(data) => <DataDisplay data={data} />}
  </QueryWrapper>
);
```

### 3. Provide User-Friendly Error Messages

Always use the error handling utilities to ensure consistent, user-friendly messages:

```tsx
// ✅ Good - uses useErrorHandler
const { handleError } = useErrorHandler({
  component: 'AdapterForm',
  operation: 'createAdapter',
});

try {
  await createAdapter(data);
} catch (error) {
  handleError(error); // Automatically extracts user-friendly message
}

// ❌ Bad - shows raw error
try {
  await createAdapter(data);
} catch (error) {
  toast.error(error.toString()); // May show cryptic error
}
```

### 4. Add Context to Errors

Provide context to help with debugging:

```tsx
const { handleError } = useErrorHandler({
  component: 'TrainingWizard',
  operation: 'submitTrainingJob',
  severity: 'error',
  onError: (error) => {
    // Custom error handling
    logger.error('Training job submission failed', {
      datasetId,
      adapterId,
    });
  },
});
```

### 5. Handle Different Severity Levels

Use appropriate severity levels for different error types:

```tsx
// Critical errors - application breaking
<SectionErrorBoundary sectionName="Core Service" severity="error">
  <CoreComponent />
</SectionErrorBoundary>

// Non-critical errors - degraded functionality
<SectionErrorBoundary sectionName="Optional Widget" severity="warning">
  <OptionalWidget />
</SectionErrorBoundary>
```

## Error Logging

All error boundaries and handlers automatically log errors to:

1. **Console** - For development debugging
2. **UI Error Store** - For in-app error tracking
3. **Logger** - For structured logging with context

Error logs include:
- Component/page name
- Operation being performed
- Error message and stack trace
- Request ID (if available)
- HTTP status code (if applicable)
- Timestamp

## Testing Error Boundaries

To test error boundaries in development:

1. Add a component that throws an error:
```tsx
function BuggyComponent() {
  throw new Error('Test error');
}
```

2. Wrap it in the error boundary you want to test:
```tsx
<SectionErrorBoundary sectionName="Test">
  <BuggyComponent />
</SectionErrorBoundary>
```

3. Verify the error UI appears and recovery options work

## Migration Guide

To add error handling to existing pages:

1. **Wrap the page component** with `withPageErrorBoundary`:
```tsx
// Before
export default function MyPage() { ... }

// After
function MyPage() { ... }
export default withPageErrorBoundary(MyPage, { pageName: 'My Page' });
```

2. **Add section boundaries** for major sections:
```tsx
<SectionErrorBoundary sectionName="Data Table">
  <DataTable />
</SectionErrorBoundary>
```

3. **Use QueryWrapper** for data fetching:
```tsx
<QueryWrapper query={myQuery}>
  {(data) => <Display data={data} />}
</QueryWrapper>
```

4. **Add error handlers** to mutations:
```tsx
const { handleError } = useErrorHandler({
  component: 'MyPage',
  operation: 'saveData',
});

const mutation = useMutation({
  mutationFn: saveData,
  onError: handleError,
});
```

## Common Patterns

### Pattern 1: Page with Multiple Tabs

```tsx
function MyPage() {
  return (
    <Tabs>
      <TabsContent value="tab1">
        <SectionErrorBoundary sectionName="Tab 1">
          <Tab1Content />
        </SectionErrorBoundary>
      </TabsContent>

      <TabsContent value="tab2">
        <SectionErrorBoundary sectionName="Tab 2">
          <Tab2Content />
        </SectionErrorBoundary>
      </TabsContent>
    </Tabs>
  );
}

export default withPageErrorBoundary(MyPage, { pageName: 'My Page' });
```

### Pattern 2: Modal with Form

```tsx
<Dialog open={open} onOpenChange={setOpen}>
  <ModalErrorBoundary onClose={() => setOpen(false)}>
    <DialogContent>
      <QueryWrapper query={formDataQuery}>
        {(data) => <MyForm initialData={data} />}
      </QueryWrapper>
    </DialogContent>
  </ModalErrorBoundary>
</Dialog>
```

### Pattern 3: Page with Multiple Queries

```tsx
function DataPage() {
  const adaptersQuery = useQuery(['adapters'], fetchAdapters);
  const modelsQuery = useQuery(['models'], fetchModels);

  return (
    <MultiQueryWrapper
      queries={[
        { query: adaptersQuery, name: 'Adapters' },
        { query: modelsQuery, name: 'Models' },
      ]}
    >
      {() => (
        <div>
          <AdaptersList adapters={adaptersQuery.data!} />
          <ModelsList models={modelsQuery.data!} />
        </div>
      )}
    </MultiQueryWrapper>
  );
}

export default withPageErrorBoundary(DataPage, { pageName: 'Data' });
```

## Troubleshooting

### Error Boundary Not Catching Errors

Error boundaries only catch errors during:
- Rendering
- Lifecycle methods
- Constructors

They do NOT catch:
- Event handlers (use try/catch)
- Async code (use try/catch or error handlers)
- Server-side rendering
- Errors in the error boundary itself

For async/event errors, use `useErrorHandler`:

```tsx
const { handleError } = useErrorHandler({ component: 'MyComponent' });

const handleClick = async () => {
  try {
    await doSomething();
  } catch (error) {
    handleError(error);
  }
};
```

### Infinite Error Loops

If an error boundary keeps triggering:

1. Check if the fallback component itself throws an error
2. Verify error state is properly reset
3. Add logging to identify the source
4. Use a parent error boundary as a safety net

## Related Documentation

- [React Error Boundaries](https://react.dev/reference/react/Component#catching-rendering-errors-with-an-error-boundary)
- [TanStack Query Error Handling](https://tanstack.com/query/latest/docs/react/guides/query-functions#handling-and-throwing-errors)
- AdapterOS logging conventions (see `ui/src/utils/logger.ts`)
