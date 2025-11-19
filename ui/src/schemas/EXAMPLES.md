# Schema Integration Examples

Practical examples of using Zod schemas in AdapterOS UI forms.

## Example 1: Simple Form Validation

### Basic Validation with Error Display

```typescript
import { DatasetConfigSchema, formatValidationError } from '@/schemas';
import { ZodError } from 'zod';

function CreateDatasetForm() {
  const [formData, setFormData] = useState({
    name: '',
    description: '',
    strategy: 'identity',
    maxSequenceLength: 2048,
    validationSplit: 0.1,
  });

  const [errors, setErrors] = useState<Record<string, string>>({});

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    try {
      // Validate against schema
      const validated = await DatasetConfigSchema.parseAsync(formData);

      // Validation passed, submit data
      await apiClient.createDataset(validated);
      toast.success('Dataset created successfully');
    } catch (error) {
      if (error instanceof ZodError) {
        // Format and display errors
        const result = formatValidationError(error);
        setErrors(result.fieldErrors);
      }
    }
  };

  return (
    <form onSubmit={handleSubmit}>
      <div>
        <label>Dataset Name</label>
        <input
          value={formData.name}
          onChange={(e) => setFormData({ ...formData, name: e.target.value })}
          aria-invalid={!!errors.name}
          aria-describedby={errors.name ? 'name-error' : undefined}
        />
        {errors.name && <span id="name-error" className="error">{errors.name}</span>}
      </div>

      <div>
        <label>Training Strategy</label>
        <select
          value={formData.strategy}
          onChange={(e) => setFormData({ ...formData, strategy: e.target.value as any })}
        >
          <option value="identity">Identity (Unsupervised)</option>
          <option value="question_answer">Question & Answer</option>
          <option value="masked_lm">Masked Language Model</option>
        </select>
      </div>

      <button type="submit">Create Dataset</button>
    </form>
  );
}
```

## Example 2: Real-Time Field Validation

### Using useFormValidation Hook

```typescript
import { useFormValidation } from '@/hooks/useFormValidation';
import { TrainingConfigSchema } from '@/schemas';

function TrainingForm() {
  const [rank, setRank] = useState(8);
  const [alpha, setAlpha] = useState(16);

  const { validateField, getFieldError } = useFormValidation(
    TrainingConfigSchema,
    {
      realtime: true,
      debounceDelay: 300,
    }
  );

  return (
    <div>
      <div>
        <label>Rank</label>
        <input
          type="number"
          value={rank}
          onChange={(e) => {
            const val = parseInt(e.target.value);
            setRank(val);
            validateField('rank', val);
          }}
          aria-invalid={!!getFieldError('rank')}
        />
        {getFieldError('rank') && (
          <span className="error">{getFieldError('rank')}</span>
        )}
        <p className="hint">LoRA rank dimension (typically 4-32)</p>
      </div>

      <div>
        <label>Alpha</label>
        <input
          type="number"
          value={alpha}
          onChange={(e) => {
            const val = parseInt(e.target.value);
            setAlpha(val);
            validateField('alpha', val);
          }}
          aria-invalid={!!getFieldError('alpha')}
        />
        {getFieldError('alpha') && (
          <span className="error">{getFieldError('alpha')}</span>
        )}
        <p className="hint">LoRA scaling factor (typically 2x rank)</p>
      </div>
    </div>
  );
}
```

## Example 3: Batch Validation

### Validate Multiple Items

```typescript
import { BatchPromptSchema } from '@/schemas';
import { ZodError } from 'zod';

async function validatePromptBatch(prompts: string[]) {
  const results = await Promise.all(
    prompts.map(async (prompt, index) => {
      try {
        await BatchPromptSchema.parseAsync({ prompt });
        return { index, valid: true };
      } catch (error) {
        if (error instanceof ZodError) {
          return {
            index,
            valid: false,
            error: error.issues[0].message,
          };
        }
        return { index, valid: false, error: 'Unknown error' };
      }
    })
  );

  return results;
}

// Usage
const prompts = [
  'What is machine learning?',
  'x',  // Too short - will fail
  'Explain deep learning in detail...',
];

const validationResults = await validatePromptBatch(prompts);

validationResults.forEach(({ index, valid, error }) => {
  if (!valid) {
    console.log(`Prompt ${index + 1}: ${error}`);
  }
});
```

## Example 4: Integration with react-hook-form

### Using useZodFormValidation Hook

```typescript
import { useForm } from 'react-hook-form';
import { useZodFormValidation } from '@/hooks/useZodFormValidation';
import { InferenceRequestSchema } from '@/schemas';

function InferenceForm() {
  const { control, handleSubmit, setError, watch } = useForm({
    defaultValues: {
      prompt: '',
      max_tokens: 100,
      temperature: 0.7,
    },
  });

  const { validateWithForm } = useZodFormValidation(
    InferenceRequestSchema,
    { setError }
  );

  const onSubmit = async (data) => {
    const result = await validateWithForm(data);

    if (!result.success) {
      // Errors already set in react-hook-form
      return;
    }

    // All validation passed
    await executeInference(data);
  };

  return (
    <form onSubmit={handleSubmit(onSubmit)}>
      {/* Form fields */}
      <textarea
        {...control.register('prompt', {
          required: true,
          minLength: 3,
        })}
        placeholder="Enter your prompt"
      />

      <button type="submit">Generate</button>
    </form>
  );
}
```

## Example 5: Complex Form with Cross-Field Validation

### TrainingWizard Integration

```typescript
import { TrainingConfigSchema, formatValidationError } from '@/schemas';

async function submitTrainingConfig(state: WizardState) {
  try {
    // Schema includes cross-field validation
    // Will check that data source type has required fields
    const validated = await TrainingConfigSchema.parseAsync({
      name: state.name,
      category: state.category,
      dataSourceType: state.dataSourceType,
      templateId: state.templateId,  // Required if dataSourceType='template'
      repositoryId: state.repositoryId,  // Required if dataSourceType='repository'
      directoryRoot: state.directoryRoot,  // Required if dataSourceType='directory'
      datasetPath: state.datasetPath,  // Required if dataSourceType='custom'
      // ... all other fields
    });

    // Validation passed - all cross-field rules satisfied
    const job = await apiClient.startTraining(validated);
    return job;
  } catch (error) {
    if (error instanceof ZodError) {
      const result = formatValidationError(error);

      // Find the first meaningful error
      const firstError = result.errors.find(e => e.suggestion);

      return {
        success: false,
        message: firstError?.message,
        suggestion: firstError?.suggestion,
      };
    }
    throw error;
  }
}
```

## Example 6: Custom Error Display

### With Suggestions

```typescript
import { formatFieldError } from '@/schemas/utils';

function FormFieldWithValidation({
  label,
  value,
  onChange,
  fieldName,
  schema,
  errors,
}) {
  const error = errors[fieldName];
  const suggestion = error?.suggestion;

  return (
    <div className="form-field">
      <label>{label}</label>
      <input
        value={value}
        onChange={onChange}
        aria-invalid={!!error}
        aria-describedby={error ? `${fieldName}-error` : undefined}
      />

      {error && (
        <div id={`${fieldName}-error`} className="error-container">
          <p className="error-message">{error.message}</p>
          {suggestion && (
            <p className="error-suggestion">
              <strong>Tip:</strong> {suggestion}
            </p>
          )}
        </div>
      )}
    </div>
  );
}

// Usage
function MyForm() {
  const [errors, setErrors] = useState<Record<string, any>>({});

  // ... form logic

  return (
    <FormFieldWithValidation
      label="Rank"
      fieldName="rank"
      value={rank}
      onChange={(e) => setRank(parseInt(e.target.value))}
      schema={TrainingConfigSchema}
      errors={errors}
    />
  );
}
```

## Example 7: Async Validation

### Validate on Blur

```typescript
async function validateOnBlur(
  fieldName: string,
  value: any,
  schema: ZodSchema
) {
  try {
    const fieldSchema = (schema as any)._shape?.[fieldName];
    if (!fieldSchema) return { valid: true };

    await fieldSchema.parseAsync(value);
    return { valid: true };
  } catch (error) {
    if (error instanceof ZodError) {
      return {
        valid: false,
        error: error.issues[0].message,
      };
    }
    return { valid: true };
  }
}

// Usage in form
function ValidationAwareInput() {
  const [value, setValue] = useState('');
  const [error, setError] = useState<string>();

  const handleBlur = async () => {
    const result = await validateOnBlur('name', value, TrainingConfigSchema);
    if (!result.valid) {
      setError(result.error);
    } else {
      setError(undefined);
    }
  };

  return (
    <div>
      <input value={value} onChange={(e) => setValue(e.target.value)} onBlur={handleBlur} />
      {error && <span className="error">{error}</span>}
    </div>
  );
}
```

## Example 8: Validating Entire Form State

### Multi-Step Wizard

```typescript
import { TrainingConfigSchema } from '@/schemas';

function MultiStepWizard() {
  const [currentStep, setCurrentStep] = useState(0);
  const [formState, setFormState] = useState({
    // Step 1
    category: null,
    name: '',
    // Step 2
    dataSourceType: 'template',
    templateId: '',
    // ... more fields
  });

  const validateAndAdvance = async () => {
    try {
      // Validate all current data
      const result = await TrainingConfigSchema.parseAsync(formState);

      // Only advance if validation passes
      setCurrentStep(currentStep + 1);
    } catch (error) {
      if (error instanceof ZodError) {
        const result = formatValidationError(error);

        // Show first error
        toast.error(result.errors[0]?.message);
        return;
      }
    }
  };

  return (
    <div>
      {/* Step content */}
      <button onClick={validateAndAdvance}>Next Step</button>
    </div>
  );
}
```

## Example 9: Type-Safe Form Data

### Leveraging Inferred Types

```typescript
import { TrainingConfigSchema, type TrainingConfigFormData } from '@/schemas';

// TypeScript automatically infers the correct type
function submitTraining(data: TrainingConfigFormData) {
  // data is type-safe and matches schema exactly
  console.log(data.rank); // number
  console.log(data.name); // string
  console.log(data.targets); // string[]
  console.log(data.category); // 'code' | 'framework' | 'codebase' | 'ephemeral'
}

// Usage with inferred types
const formData: TrainingConfigFormData = {
  name: 'my-adapter',
  category: 'code',
  // ... TypeScript will error if required fields are missing
};

await submitTraining(formData);
```

## Example 10: Error Recovery

### Handling Validation Failures

```typescript
import { useFormValidation } from '@/hooks/useFormValidation';
import { DatasetConfigSchema } from '@/schemas';

function DatasetForm() {
  const { validate, validationResult, clearErrors } = useFormValidation(
    DatasetConfigSchema
  );

  const handleSubmit = async (data) => {
    const result = await validate(data);

    if (!result.success) {
      // Show error summary
      const errorCount = result.errors.length;
      toast.error(`${errorCount} validation error${errorCount !== 1 ? 's' : ''}`);

      // Log for debugging
      console.error('Validation errors:', result.fieldErrors);

      // Focus first error field
      const firstErrorField = Object.keys(result.fieldErrors)[0];
      document.getElementById(firstErrorField)?.focus();

      return;
    }

    // Success
    try {
      await createDataset(data);
      clearErrors(); // Reset after successful submit
    } catch (error) {
      // Handle API errors separately
    }
  };

  return (
    <form onSubmit={() => handleSubmit(/* data */)}>
      {/* Form fields */}
    </form>
  );
}
```

## Best Practices

1. **Always use async validation** for proper error handling
2. **Combine real-time and submit validation** for best UX
3. **Display suggestions** alongside error messages
4. **Log validation failures** for debugging
5. **Use TypeScript inference** for type safety
6. **Debounce real-time validation** to avoid excessive checks
7. **Validate before API calls** to save bandwidth
8. **Handle ZodError specifically** for validation vs runtime errors
9. **Provide clear field-level feedback** with ARIA attributes
10. **Test validation rules** separately from component logic
