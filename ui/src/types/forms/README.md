# Form Types

Comprehensive TypeScript type definitions for all form-related state and validation in the AdapterOS UI.

## Structure

```
ui/src/types/forms/
├── adapter-form.ts      # Adapter creation, edit, registration, import forms
├── admin-form.ts        # User, tenant, policy, workspace forms
├── auth-form.ts         # Login, registration, password reset, TOTP forms
├── common.ts            # Generic form component props and utilities
├── index.ts             # Central export
├── inference-form.ts    # Inference, batch inference, chat forms
└── training-form.ts     # Training config, dataset config, wizard forms
```

## Usage

### Import Form Types

```typescript
import type {
  TrainingConfigFormData,
  TrainingFormState,
  AdapterCreateFormData,
  LoginFormData,
  FormValidationState,
} from '@/types/forms';
```

### With React Hook Form

```typescript
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import type { TrainingConfigFormData } from '@/types/forms';
import { TrainingConfigSchema } from '@/schemas/forms';

function TrainingForm() {
  const form = useForm<TrainingConfigFormData>({
    resolver: zodResolver(TrainingConfigSchema),
    defaultValues: {
      name: '',
      category: 'code',
      rank: 8,
      alpha: 16,
      epochs: 3,
      learningRate: 0.0001,
      batchSize: 4,
      targets: ['q_proj', 'v_proj'],
    },
  });

  const onSubmit = async (data: TrainingConfigFormData) => {
    // Submit training job
  };

  return <form onSubmit={form.handleSubmit(onSubmit)}>...</form>;
}
```

## Type Categories

### Training Forms (`training-form.ts`)

- **TrainingConfigFormData**: Complete training configuration
- **SemanticTrainingFormData**: Semantic naming format (tenant/domain/purpose/revision)
- **TrainingWizardFormState**: Multi-step wizard state
- **DatasetConfigFormData**: Dataset configuration

### Adapter Forms (`adapter-form.ts`)

- **AdapterCreateFormData**: New adapter creation
- **AdapterEditFormData**: Editing existing adapter
- **AdapterRegisterFormData**: Registering adapter from file
- **AdapterImportFormData**: Importing adapter from URL
- **StackFormData**: Adapter stack configuration with Q15 gates

### Auth Forms (`auth-form.ts`)

- **LoginFormData**: Email, password, TOTP
- **RegistrationFormData**: New user registration
- **PasswordResetRequestFormData**: Request password reset
- **TotpSetupFormData**: TOTP 2FA setup

### Admin Forms (`admin-form.ts`)

- **UserFormData**: User creation/editing
- **TenantFormData**: Tenant configuration
- **PolicyConfigFormData**: Policy customization
- **WorkspaceFormData**: Workspace settings

### Inference Forms (`inference-form.ts`)

- **InferenceRequestFormData**: Single inference request
- **BatchInferenceFormState**: Batch operations with progress
- **ChatMessageFormData**: Chat interface
- **SamplingParametersFormData**: Temperature, top_k, top_p settings

## Validation State Pattern

All form types follow a consistent validation state pattern:

```typescript
interface FormValidationState {
  isValid: boolean;
  errors: Record<string, string>;
  touched: Record<string, boolean>;
  isSubmitting?: boolean;
  warnings?: Record<string, string>;
}
```

## Complete Form State Pattern

Many forms combine data and validation:

```typescript
interface TrainingFormState {
  config: TrainingConfigFormData;
  validation: TrainingFormValidationState;
  step?: number;
  totalSteps?: number;
}
```

## Schema Alignment

Form types are derived from Zod schemas in `ui/src/schemas/`:

- `forms.ts` - Training, dataset, inference, batch
- `admin.schema.ts` - Stack, tenant, user
- `common.schema.ts` - Login, pagination, common validators

Use `z.infer<typeof Schema>` to generate types from schemas:

```typescript
import { z } from 'zod';
import { TrainingConfigSchema } from '@/schemas/forms';

export type TrainingConfigFormData = z.infer<typeof TrainingConfigSchema>;
```

## Common Patterns

### Multi-Step Forms

```typescript
interface TrainingWizardFormState {
  currentStep: number;
  steps: TrainingWizardStep[];
  formData: Partial<TrainingConfigFormData>;
  canGoNext: boolean;
  canGoPrevious: boolean;
  canSubmit: boolean;
}
```

### Edit Forms with Change Tracking

```typescript
interface AdapterEditFormState {
  formData: AdapterEditFormData;
  validation: AdapterFormValidationState;
  originalData?: AdapterEditFormData;
  hasChanges: boolean;
}
```

### Forms with Progress

```typescript
interface BatchInferenceFormState {
  prompts: BatchPromptFormData[];
  commonSettings: { /* ... */ };
  validation: InferenceFormValidationState;
  progress?: {
    current: number;
    total: number;
  };
}
```

## Best Practices

1. **Use Zod schemas** for validation and type generation
2. **Separate data from validation state** for clarity
3. **Include metadata** (step, progress, hasChanges) when relevant
4. **Follow naming conventions**:
   - `*FormData` - Raw form field values
   - `*FormState` - Complete form state with validation
   - `*FormValidationState` - Validation-specific state

## Related Files

- `/ui/src/schemas/` - Zod validation schemas
- `/ui/src/types/state/` - Generic state management types
- `/ui/src/hooks/forms/` - Form hooks (useForm, useValidation, etc.)
- `/ui/src/components/shared/Form/` - Reusable form components
