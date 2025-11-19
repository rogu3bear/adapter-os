# Form Validation Integration Guide

This document describes how Zod schemas are integrated into AdapterOS UI forms for comprehensive validation.

## Overview

Zod schemas provide runtime type-safe validation for all major forms:
- **TrainingWizard** - Training configuration and adapter parameters
- **DatasetBuilder** - Dataset configuration and file uploads
- **InferencePlayground** - Inference requests and batch processing
- **PromotionWorkflow** - Promotion requests and approval justifications

## Schemas

### 1. TrainingConfigSchema
Validates all training parameters across all steps of the training wizard.

**Usage:**
```typescript
import { TrainingConfigSchema } from '@/schemas';

try {
  const validated = await TrainingConfigSchema.parseAsync(formData);
  // Validation passed, proceed with training
} catch (error) {
  if (error instanceof ZodError) {
    // Handle validation errors
    const formatted = formatValidationError(error);
  }
}
```

**Fields Validated:**
- Basic info: `name`, `description`, `category`, `scope`
- Data source: `dataSourceType`, `templateId`, `repositoryId`, etc.
- Category-specific: `language`, `frameworkId`, `repoScope`, etc.
- Training: `rank`, `alpha`, `epochs`, `learningRate`, `batchSize`, `targets`
- Packaging: `packageAfter`, `registerAfter`, `adaptersRoot`, `tier`

**Cross-field Validation:**
- Template data source requires `templateId`
- Repository data source requires `repositoryId`
- Directory data source requires `directoryRoot`
- Custom data source requires `datasetPath`
- Code category requires `language`

### 2. DatasetConfigSchema
Validates dataset configuration for the DatasetBuilder.

**Usage:**
```typescript
import { DatasetConfigSchema } from '@/schemas';

const validated = await DatasetConfigSchema.parseAsync(config);
```

**Fields Validated:**
- `name` - Dataset name (3-100 chars)
- `description` - Optional description (max 500 chars)
- `strategy` - Training strategy (identity, question_answer, masked_lm)
- `maxSequenceLength` - Sequence length (128-8192)
- `validationSplit` - Train/validation split (0-0.5)
- `tokenizer` - Optional tokenizer ID

### 3. InferenceRequestSchema
Validates inference parameters for the InferencePlayground.

**Usage:**
```typescript
import { InferenceRequestSchema } from '@/schemas';

const validated = await InferenceRequestSchema.parseAsync({
  prompt: userPrompt,
  max_tokens: 100,
  temperature: 0.7,
  top_k: 50,
  top_p: 0.9,
});
```

**Fields Validated:**
- `prompt` - Required, 3-50000 chars, no invisible Unicode
- `max_tokens` - 10-2000 tokens
- `temperature` - 0-2
- `top_k` - 1-100
- `top_p` - 0-1
- `seed` - Optional integer
- `require_evidence` - Boolean
- `adapters` - Optional adapter array

### 4. PromotionRequestSchema
Validates promotion workflow parameters.

**Usage:**
```typescript
import { PromotionRequestSchema } from '@/schemas';

const validated = await PromotionRequestSchema.parseAsync(approvalData);
```

**Fields Validated:**
- `stage_id` - Required stage identifier
- `justification` - 10-2000 character justification
- `target_environment` - staging or production
- `rollbackPlan` - Optional rollback plan with email validation
- `approver` - Optional approver name
- `approved_at` - Optional approval timestamp

### 5. BatchPromptSchema
Validates individual prompts in batch operations.

**Usage:**
```typescript
import { BatchPromptSchema } from '@/schemas';

const validations = await Promise.all(
  prompts.map(p => BatchPromptSchema.parseAsync({ prompt: p }))
);
```

## Utility Functions

### formatValidationError
Convert ZodError to user-friendly format.

```typescript
import { formatValidationError } from '@/schemas/utils';

try {
  await schema.parseAsync(data);
} catch (error) {
  if (error instanceof ZodError) {
    const result = formatValidationError(error);
    // result.success: boolean
    // result.errors: ValidationErrorDetail[]
    // result.fieldErrors: Record<string, string>
  }
}
```

### validateField
Validate a single field in real-time.

```typescript
import { validateField } from '@/schemas/utils';

const result = validateField(schema, 'rank', 8);
// result.valid: boolean
// result.error?: string
// result.suggestion?: string
```

### formatFieldError
Format a validation error with suggestion.

```typescript
import { formatFieldError } from '@/schemas/utils';

const message = formatFieldError('rank', 'too small', 'Increase rank to at least 2');
// Output: "rank: too small (Tip: Increase rank to at least 2)"
```

## Hooks

### useFormValidation
Real-time validation hook for forms.

```typescript
import { useFormValidation } from '@/hooks/useFormValidation';

function MyForm() {
  const {
    validate,           // Validate entire form
    validateField,      // Validate single field
    getFieldError,      // Get error for field
    clearErrors,        // Clear all errors
    hasErrors,          // Check if has errors
  } = useFormValidation(TrainingConfigSchema, {
    realtime: true,           // Enable real-time validation
    debounceDelay: 300,       // Debounce delay in ms
  });

  const handleSubmit = async (data) => {
    const result = await validate(data);
    if (!result.success) {
      // Show errors
      return;
    }
    // Submit data
  };

  return (
    <input
      onChange={(e) => validateField('name', e.target.value)}
      aria-invalid={!!getFieldError('name')}
    />
  );
}
```

### useZodFormValidation
Integration hook for react-hook-form.

```typescript
import { useZodFormValidation } from '@/hooks/useZodFormValidation';
import { useForm } from 'react-hook-form';

function MyForm() {
  const { handleSubmit, setError } = useForm();
  const { validateWithForm } = useZodFormValidation(schema, { setError });

  const onSubmit = async (data) => {
    const result = await validateWithForm(data);
    if (!result.success) {
      // react-hook-form errors already set
      return;
    }
    // Submit data
  };

  return <form onSubmit={handleSubmit(onSubmit)}>{/* ... */}</form>;
}
```

## Error Messages

### Custom Error Messages

Schemas include user-friendly error messages for each validation rule:

```typescript
// Example error messages
"Name must be at least 3 characters"
"Max sequence length must be at least 128"
"Validation split must be between 0 and 0.5"
"Prompt contains unsupported control or invisible characters"
```

### Suggestions

Many errors include helpful suggestions:

```typescript
// Example suggestion
"Increase rank to at least 2 for better model adaptation"
"Consider breaking your prompt into smaller chunks"
"Remove any invisible characters or control characters"
```

## Integration Examples

### TrainingWizard Integration

```typescript
import { TrainingConfigSchema } from '@/schemas';

const handleComplete = async () => {
  try {
    // Validate all form data
    const validated = await TrainingConfigSchema.parseAsync({
      name: state.name,
      category: state.category,
      rank: state.rank,
      // ... all fields
    });

    // Validation passed, proceed with training
    const job = await apiClient.startTraining(validated);
  } catch (error) {
    if (error instanceof ZodError) {
      const result = formatValidationError(error);
      setValidationError(result.errors[0]?.message);
    }
  }
};
```

### InferencePlayground Integration

```typescript
import { InferenceRequestSchema, BatchPromptSchema } from '@/schemas';

const handleInfer = async (config) => {
  try {
    // Validate inference request
    await InferenceRequestSchema.parseAsync(config);
    // Proceed with inference
  } catch (error) {
    // Handle validation error
  }
};

const executeBatchInference = async (prompts) => {
  // Validate each prompt
  const validations = await Promise.all(
    prompts.map(p =>
      BatchPromptSchema.parseAsync({ prompt: p })
        .then(() => ({ valid: true }))
        .catch(error => ({ valid: false, error: error.message }))
    )
  );
};
```

### DatasetBuilder Integration

```typescript
import { DatasetConfigSchema } from '@/schemas';

const createDataset = async () => {
  try {
    // Validate dataset config
    await DatasetConfigSchema.parseAsync(config);
    // Validation passed
    const datasetId = await apiClient.createDataset(config);
    onDatasetCreated?.(datasetId, config);
  } catch (error) {
    if (error instanceof ZodError) {
      const result = formatValidationError(error);
      setUploadError(new Error(
        result.errors.map(e => e.message).join('\n')
      ));
    }
  }
};
```

## Best Practices

1. **Always use async validation** for forms with multiple fields:
   ```typescript
   await schema.parseAsync(data);  // Recommended
   schema.parse(data);             // Avoid - synchronous
   ```

2. **Catch and format Zod errors**:
   ```typescript
   try {
     // validation
   } catch (error) {
     if (error instanceof ZodError) {
       const result = formatValidationError(error);
       // Use result.errors or result.fieldErrors
     }
   }
   ```

3. **Use real-time validation for better UX**:
   ```typescript
   const { validateField, getFieldError } = useFormValidation(schema, {
     realtime: true,
     debounceDelay: 300,
   });
   ```

4. **Display suggestions alongside errors**:
   ```typescript
   {error && (
     <div>
       <p className="error">{error.message}</p>
       {error.suggestion && (
         <p className="hint">{error.suggestion}</p>
       )}
     </div>
   )}
   ```

5. **Validate before API calls**:
   ```typescript
   try {
     await schema.parseAsync(data);
     await apiClient.submitData(data);
   } catch (validationError) {
     // Handle validation errors first
   }
   ```

## Testing

### Test Validation Success

```typescript
import { TrainingConfigSchema } from '@/schemas';

const validData = {
  name: 'my-adapter',
  category: 'code',
  // ... all required fields
};

const result = await TrainingConfigSchema.parseAsync(validData);
expect(result).toEqual(validData);
```

### Test Validation Failure

```typescript
import { TrainingConfigSchema } from '@/schemas';
import { ZodError } from 'zod';

const invalidData = {
  name: 'ab',  // Too short
  category: 'invalid',  // Invalid enum
};

expect(
  async () => TrainingConfigSchema.parseAsync(invalidData)
).toThrow(ZodError);
```

## Future Enhancements

1. **Async validation** - Network-based validation (e.g., unique adapter names)
2. **Conditional schemas** - Different validation based on category/type
3. **Custom error messages** - Localization support
4. **Form builder integration** - Automatic form generation from schemas
5. **OpenAPI sync** - Keep schemas in sync with API definitions
