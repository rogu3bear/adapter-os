# Validation Schemas - Quick Reference

Quick copy-paste examples for common validation scenarios.

## Import Schemas

```typescript
// Training
import {
  BackendTrainingConfigSchema,
  StartTrainingRequestSchema,
  TrainingTemplates,
} from '@/schemas';

// Adapters
import {
  AdapterNameSchema,
  RegisterAdapterRequestSchema,
  AdapterNameUtils,
  StackNameSchema,
} from '@/schemas';

// Inference
import {
  StreamingInferenceRequestSchema,
  InferencePresets,
  InferenceUtils,
} from '@/schemas';

// Common
import {
  TenantIdSchema,
  RepositoryIdSchema,
  Blake3HashSchema,
  ValidationUtils,
} from '@/schemas';
```

## Validate Adapter Name

```typescript
import { AdapterNameSchema, AdapterNameUtils } from '@/schemas';

// Basic validation
const result = AdapterNameSchema.safeParse('tenant-a/engineering/code-review/r001');

if (!result.success) {
  console.error('Invalid adapter name:', result.error.flatten());
  return;
}

// Parse components
const parsed = AdapterNameUtils.parse(result.data);
console.log({
  tenant: parsed.tenant,
  domain: parsed.domain,
  purpose: parsed.purpose,
  revision: parsed.revision,
  revisionNumber: parsed.revisionNumber,
  lineage: parsed.lineage,
});

// Get next revision
const nextName = AdapterNameUtils.nextRevision(result.data);
console.log('Next revision:', nextName);
```

## Start Training Job

```typescript
import { StartTrainingRequestSchema, TrainingTemplates } from '@/schemas';

// Use a preset template
const request = {
  adapter_name: 'tenant-a/engineering/code-review/r001',
  config: TrainingTemplates.standard.config,
  repo_id: 'myorg/myrepo',
};

// Validate before API call
const result = StartTrainingRequestSchema.safeParse(request);

if (!result.success) {
  console.error('Validation failed:', result.error.flatten());
  return;
}

// Make API call
const response = await fetch('/api/training/start', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(result.data),
});
```

## Register Adapter

```typescript
import { RegisterAdapterRequestSchema, SupportedLanguages } from '@/schemas';

const request = {
  adapter_id: 'adapter-12345',
  name: 'tenant-a/engineering/code-review/r001',
  hash_b3: 'b3:' + '0'.repeat(64),
  rank: 16,
  tier: 1,
  languages: ['python', 'rust'],
  framework: 'pytorch',
};

// Validate
const result = RegisterAdapterRequestSchema.safeParse(request);

if (!result.success) {
  console.error('Validation failed:', result.error.flatten());
  return;
}

// Make API call
const response = await fetch('/api/adapters/register', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(result.data),
});
```

## Create Adapter Stack

```typescript
import { CreateAdapterStackRequestSchema } from '@/schemas';

const request = {
  name: 'stack.production-env',
  description: 'Production environment stack',
  adapter_ids: [
    'adapter-1',
    'adapter-2',
    'adapter-3',
  ],
  workflow_type: 'code-review',
};

// Validate
const result = CreateAdapterStackRequestSchema.safeParse(request);

if (!result.success) {
  console.error('Validation failed:', result.error.flatten());
  return;
}

// Make API call
const response = await fetch('/api/adapter-stacks', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(result.data),
});
```

## Streaming Inference

```typescript
import { StreamingInferenceRequestSchema, InferencePresets } from '@/schemas';

// Use a preset
const request = {
  prompt: 'Explain quantum computing',
  ...InferencePresets.balanced.config,
  stream: true,
  adapter_stack: 'stack.production-env',
};

// Validate
const result = StreamingInferenceRequestSchema.safeParse(request);

if (!result.success) {
  console.error('Validation failed:', result.error.flatten());
  return;
}

// Make API call (streaming)
const response = await fetch('/api/chat/completions', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(result.data),
});
```

## Form Validation (React Hook Form)

```typescript
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { TrainingConfigSchema } from '@/schemas';

function TrainingForm() {
  const form = useForm({
    resolver: zodResolver(TrainingConfigSchema),
    defaultValues: {
      name: '',
      category: 'code',
      scope: 'global',
      dataSourceType: 'repository',
      rank: 16,
      alpha: 32,
      epochs: 3,
      learningRate: 0.001,
      batchSize: 32,
      targets: ['q_proj', 'v_proj'],
    },
  });

  const onSubmit = (data: TrainingConfigFormData) => {
    console.log('Valid data:', data);
    // Call API...
  };

  return (
    <form onSubmit={form.handleSubmit(onSubmit)}>
      <input {...form.register('name')} />
      {form.formState.errors.name && (
        <span className="text-red-500">
          {form.formState.errors.name.message}
        </span>
      )}
      {/* More fields... */}
    </form>
  );
}
```

## Validate Field Programmatically

```typescript
import { validateField, formatFieldError } from '@/schemas';
import { TenantIdSchema } from '@/schemas';

function validateTenantId(value: string) {
  const result = validateField(TenantIdSchema, value);

  if (!result.valid) {
    const errorMessage = formatFieldError('tenant_id', result.error);
    console.error(errorMessage);
    return false;
  }

  return true;
}
```

## Error Handling

```typescript
import { parseValidationErrors, formatValidationError } from '@/schemas';
import { StartTrainingRequestSchema } from '@/schemas';

const data = {
  adapter_name: 'invalid-name', // Wrong format
  config: {
    rank: 0, // Below minimum
    // Missing required fields
  },
};

const result = StartTrainingRequestSchema.safeParse(data);

if (!result.success) {
  // Get all errors
  const errors = parseValidationErrors(result.error);

  // Display to user
  errors.forEach(error => {
    const message = formatValidationError(error.path, error.message);
    toast.error(message);
  });

  // Or get flattened errors
  const flattened = result.error.flatten();
  console.log('Field errors:', flattened.fieldErrors);
  console.log('Form errors:', flattened.formErrors);
}
```

## Custom Validation with Refinements

```typescript
import { z } from 'zod';
import { AdapterNameSchema } from '@/schemas';

// Add custom validation on top of existing schema
const CustomAdapterSchema = AdapterNameSchema.refine(
  async (name) => {
    // Check if adapter already exists
    const response = await fetch(`/api/adapters/${name}`);
    return response.status === 404; // Should not exist
  },
  {
    message: 'Adapter with this name already exists',
  }
);

// Use in form
const result = await CustomAdapterSchema.safeParseAsync('tenant-a/engineering/code-review/r001');
```

## Conditional Validation

```typescript
import { z } from 'zod';

const ConditionalSchema = z.object({
  dataSourceType: z.enum(['repository', 'directory']),
  repositoryId: z.string().optional(),
  directoryRoot: z.string().optional(),
}).refine(
  (data) => {
    // If repository, require repositoryId
    if (data.dataSourceType === 'repository') {
      return !!data.repositoryId;
    }
    // If directory, require directoryRoot
    if (data.dataSourceType === 'directory') {
      return !!data.directoryRoot;
    }
    return true;
  },
  {
    message: 'Required field missing for selected data source',
    path: ['repositoryId'], // Or dynamically set path
  }
);
```

## Validate Before Mutation

```typescript
import { useMutation } from '@tanstack/react-query';
import { StartTrainingRequestSchema } from '@/schemas';

const startTrainingMutation = useMutation({
  mutationFn: async (data: unknown) => {
    // Validate before making API call
    const result = StartTrainingRequestSchema.safeParse(data);

    if (!result.success) {
      throw new Error('Validation failed: ' + JSON.stringify(result.error.flatten()));
    }

    // Make API call with validated data
    const response = await fetch('/api/training/start', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(result.data),
    });

    if (!response.ok) {
      throw new Error('API call failed');
    }

    return response.json();
  },
});

// Use in component
const handleStartTraining = () => {
  startTrainingMutation.mutate(formData);
};
```

## Validate User Input (Debounced)

```typescript
import { useState, useEffect } from 'react';
import { AdapterNameSchema } from '@/schemas';
import { useDebounce } from '@/hooks/useDebounce';

function AdapterNameInput() {
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);
  const debouncedName = useDebounce(name, 500);

  useEffect(() => {
    if (!debouncedName) {
      setError(null);
      return;
    }

    const result = AdapterNameSchema.safeParse(debouncedName);

    if (!result.success) {
      setError(result.error.issues[0].message);
    } else {
      setError(null);
    }
  }, [debouncedName]);

  return (
    <div>
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        className={error ? 'border-red-500' : ''}
      />
      {error && <span className="text-red-500">{error}</span>}
    </div>
  );
}
```

## Type Inference

```typescript
import { z } from 'zod';
import { StartTrainingRequestSchema } from '@/schemas';

// Infer TypeScript type from schema
type StartTrainingRequest = z.infer<typeof StartTrainingRequestSchema>;

// Use in function signature
function startTraining(request: StartTrainingRequest) {
  // TypeScript knows the exact shape
  console.log(request.adapter_name);
  console.log(request.config.rank);
}

// Or infer input type (before validation)
type StartTrainingInput = z.input<typeof StartTrainingRequestSchema>;
```

## Utility Functions

```typescript
import { ValidationUtils, InferenceUtils } from '@/schemas';

// File size formatting
const size = ValidationUtils.formatFileSize(1024 * 1024 * 5); // "5 MB"

// Relative time
const time = ValidationUtils.getRelativeTime('2024-01-19T10:00:00Z'); // "5m ago"

// Sanitize user input
const safe = ValidationUtils.sanitizeString('<script>alert("xss")</script>');
// "&lt;script&gt;alert(&quot;xss&quot;)&lt;/script&gt;"

// Estimate token count
const tokens = InferenceUtils.estimateTokenCount('Hello world'); // ~3

// Validate prompt length
const isValid = InferenceUtils.validatePromptLength('Hello', 512); // true

// Get recommended max tokens
const maxTokens = InferenceUtils.getRecommendedMaxTokens('Long prompt...');
```

## Common Patterns

### Validate and Transform

```typescript
import { z } from 'zod';

const TransformSchema = z.object({
  name: z.string().transform(val => val.toLowerCase().trim()),
  tags: z.string().transform(val => val.split(',').map(t => t.trim())),
});

const result = TransformSchema.parse({
  name: '  My Adapter  ',
  tags: 'python, rust, typescript',
});

console.log(result.name); // "my adapter"
console.log(result.tags); // ["python", "rust", "typescript"]
```

### Partial Validation

```typescript
import { StartTrainingRequestSchema } from '@/schemas';

// Validate only some fields (useful for drafts)
const PartialSchema = StartTrainingRequestSchema.partial();

const draft = {
  adapter_name: 'tenant-a/engineering/code-review/r001',
  // config is optional now
};

const result = PartialSchema.safeParse(draft);
```

### Pick/Omit Fields

```typescript
import { StartTrainingRequestSchema } from '@/schemas';

// Only validate adapter_name and config
const SimplifiedSchema = StartTrainingRequestSchema.pick({
  adapter_name: true,
  config: true,
});

// Omit optional fields
const RequiredSchema = StartTrainingRequestSchema.omit({
  template_id: true,
  repo_id: true,
  dataset_id: true,
});
```

## Testing Examples

```typescript
import { describe, it, expect } from 'vitest';
import { AdapterNameSchema, AdapterNameUtils } from '@/schemas';

describe('Adapter Name Validation', () => {
  it('validates correct names', () => {
    const result = AdapterNameSchema.safeParse('tenant-a/engineering/code-review/r001');
    expect(result.success).toBe(true);
  });

  it('rejects reserved tenants', () => {
    const result = AdapterNameSchema.safeParse('system/engineering/code-review/r001');
    expect(result.success).toBe(false);
  });

  it('parses name components', () => {
    const parsed = AdapterNameUtils.parse('tenant-a/engineering/code-review/r001');
    expect(parsed.tenant).toBe('tenant-a');
    expect(parsed.domain).toBe('engineering');
    expect(parsed.purpose).toBe('code-review');
    expect(parsed.revisionNumber).toBe(1);
  });

  it('validates revision gap', () => {
    expect(AdapterNameUtils.validateRevisionGap(1, 2)).toBe(true);
    expect(AdapterNameUtils.validateRevisionGap(1, 7)).toBe(false); // Gap > 5
  });
});
```
