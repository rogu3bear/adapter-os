# Prompt Template Integration Guide

## Overview

The InferencePlayground has been integrated with a comprehensive prompt template management system that enables users to:
- Create and manage reusable prompt templates
- Apply templates with variable substitution
- Save prompts as templates
- Quick access to recent templates
- Track template usage

## Files Added

### 1. Hook: `usePromptTemplates.ts`
**Location:** `/Users/star/Dev/aos/ui/src/hooks/usePromptTemplates.ts`

Custom React hook for template management with the following features:

```typescript
export interface PromptTemplate {
  id: string;
  name: string;
  description: string;
  prompt: string;
  category: string;
  variables: string[];
  created_at: string;
  updated_at: string;
  isFavorite?: boolean;
}
```

**Key Methods:**
- `createTemplate()` - Create new template
- `updateTemplate()` - Update existing template
- `deleteTemplate()` - Delete template
- `getTemplate()` - Retrieve single template
- `getTemplates()` - List all or filtered templates
- `getRecentTemplates()` - Get recently used templates
- `recordTemplateUsage()` - Track template usage
- `toggleFavorite()` - Mark as favorite
- `substituteVariables()` - Replace {{variable}} with values
- `searchTemplates()` - Search by name/description/content
- `getCategories()` - Get unique template categories

**Storage:**
- Persists to localStorage via `aos_prompt_templates` key
- Recent templates tracked in `aos_recent_templates` key
- Automatically detects variables using `{{variable}}` syntax

**Default Templates:**
Includes 6 built-in templates:
- Code Review
- Generate Documentation
- Bug Analysis
- Summarize Text
- Explain Concept
- Creative Writing

### 2. Component: `PromptTemplateManager.tsx`
**Location:** `/Users/star/Dev/aos/ui/src/components/PromptTemplateManager.tsx`

Dialog component providing full CRUD interface for templates:

**Features:**
- List view with search and filtering
- Category filter dropdown
- Sort options (recent, name A-Z, favorites)
- Template cards with quick actions:
  - Edit
  - Copy to clipboard
  - Delete
  - Toggle favorite
- Create/edit form with variable preview
- Automatic variable detection from template text

**Usage:**
```typescript
<PromptTemplateManager
  open={showTemplateManager}
  onOpenChange={setShowTemplateManager}
  onSelectTemplate={handleApplyTemplate}
/>
```

## Files Modified

### InferencePlayground.tsx
**Location:** `/Users/star/Dev/aos/ui/src/components/InferencePlayground.tsx`

**Integration Points:**

1. **Imports Added:**
   ```typescript
   import { PromptTemplateManager } from './PromptTemplateManager';
   import { usePromptTemplates, PromptTemplate as PromptTemplateType } from '../hooks/usePromptTemplates';
   import { Plus, Check } from 'lucide-react'; // Added icons
   ```

2. **State Management:**
   ```typescript
   // Template management
   const { recordTemplateUsage, substituteVariables, getRecentTemplates } = usePromptTemplates();
   const [showTemplateManager, setShowTemplateManager] = useState(false);
   const [selectedTemplate, setSelectedTemplate] = useState<PromptTemplateType | null>(null);
   const [templateVariables, setTemplateVariables] = useState<Record<string, string>>({});
   const [showVariableInputs, setShowVariableInputs] = useState(false);
   const [promptModifiedSinceTemplate, setPromptModifiedSinceTemplate] = useState(false);
   ```

3. **Handlers Added:**
   - `handleApplyTemplate()` - Apply selected template
   - `handleApplyVariableSubstitution()` - Substitute variables and apply
   - `handleResetToTemplate()` - Reset prompt to template
   - `handleSavePromptAsTemplate()` - Save current prompt as template

4. **Prompt Input Enhancement:**
   - Modified onChange handler to track modifications since template application
   - Added template status indicators (blue for active, yellow for modified)
   - Variable substitution inputs with real-time preview

5. **UI Components Added:**
   - Template status indicator alert (blue when using template)
   - Modified template alert (yellow when edited) with reset button
   - Template selector section with:
     - Recent templates quick access
     - View all templates button
     - Manage button (opens full template manager)
   - Variable input section:
     - Input fields for each detected variable
     - Real-time preview of substituted prompt
     - Apply/Cancel buttons
   - "Save Prompt as Template" button in configuration panel
   - PromptTemplateManager dialog at bottom of component

## State Management Flow

### Template Application Flow
```
User clicks template
    ↓
handleApplyTemplate()
    ↓
Detect variables
    ↓
If variables exist:
    Show variable input section
Else:
    Apply directly to prompt
    ↓
Show template status indicator
```

### Variable Substitution Flow
```
User enters variable values
    ↓
Real-time preview updates
    ↓
User clicks "Apply Template"
    ↓
handleApplyVariableSubstitution()
    ↓
substituteVariables() replaces {{variable}} patterns
    ↓
Prompt updated with substituted text
    ↓
Show template status indicator
```

### Modification Tracking Flow
```
User edits prompt after applying template
    ↓
onChange handler detects change
    ↓
setPromptModifiedSinceTemplate(true)
    ↓
Yellow alert shown with "Reset" button
    ↓
User can reset or continue editing
```

## User Workflows

### Workflow 1: Apply Template Without Variables
```
1. Click Templates button
2. Select recent template (e.g., "Code Review")
3. Template auto-applies to prompt textarea
4. Blue indicator shows "Using template: Code Review"
5. Click Generate to run inference
```

### Workflow 2: Apply Template With Variables
```
1. Click Templates button
2. Select template with variables (e.g., "Summarize Text")
3. Variable inputs appear with labels:
   - {{length}}
   - {{focus}}
   - {{text}}
4. Enter values in each field
5. Real-time preview shows substituted prompt
6. Click "Apply Template"
7. Prompt updated with variable values
8. Blue indicator shows "Using template: Summarize Text (3 variables)"
```

### Workflow 3: Create Custom Template
```
1. Click Templates button
2. Click "Manage" button
3. Click "New Template"
4. Enter:
   - Name: "My Code Analyzer"
   - Category: "engineering"
   - Description: "Analyze code patterns"
   - Prompt: "Analyze this code: {{code}}\nLook for: {{patterns}}"
5. Variable preview shows: [code] [patterns]
6. Click "Save Template"
7. Template available in manager and quick access
```

### Workflow 4: Save Prompt as Template
```
1. Write prompt in textarea
2. Click "Save Prompt as Template" button
3. Template manager opens in create mode
4. Pre-filled with:
   - Prompt text from textarea
   - Auto-detected variables
5. Enter name, description, category
6. Click "Save Template"
7. Prompt added to template library
```

### Workflow 5: Modify and Reset
```
1. Apply template to prompt
2. Edit prompt (modify text)
3. Yellow alert appears: "Prompt has been modified from template"
4. Click "Reset" to restore original template text
5. OR continue editing and ignore alert
```

## Component Integration Points

### Template Manager Dialog
Located at bottom of InferencePlayground return statement:
```typescript
<PromptTemplateManager
  open={showTemplateManager}
  onOpenChange={setShowTemplateManager}
  onSelectTemplate={handleApplyTemplate}
/>
```

### Template Selection UI (Single Mode Only)
Located in Configuration panel, after prompt textarea:
- Template status indicators
- Template selector with recent templates
- Variable input fields when needed

### Buttons
1. **Templates Toggle** - Shows/hides template selector
2. **Manage** - Opens full template manager dialog
3. **View All Templates** - Opens full template manager dialog
4. **Apply Template** - Applies template with substituted variables
5. **Reset** - Resets prompt to original template text (when modified)
6. **Save Prompt as Template** - Opens template manager in create mode

## Storage & Persistence

### localStorage Keys
- `aos_prompt_templates` - All templates (JSON array)
- `aos_recent_templates` - Last 5 used templates (JSON array of IDs)

### Auto-Save
- Templates auto-save on create/update/delete
- Recent templates auto-update on usage
- No server-side persistence (local only)

### Error Handling
- Graceful fallback to defaults if storage corrupted
- Errors logged via `logger` utility
- User-facing notifications via toast

## Variable Substitution

### Syntax
- Supports `{{variable}}` (double braces)
- Also supports `{variable}` (single braces) for compatibility

### Detection
Automatic detection using regex:
```javascript
/\{\{(\w+)\}\}/g  // Double braces
/\{(\w+)\}/g      // Single braces
```

### Preview
Real-time preview shown while entering variable values:
```
Input: {{code}}
Preview: [shows substituted text as user types]
```

## UI/UX Features

### Visual Feedback
1. **Blue Alert** - Shows when template is active and unmodified
2. **Yellow Alert** - Shows when prompt modified from template
3. **Badge** - Shows variable count
4. **Recent Templates** - Quick access without opening manager

### Non-Intrusive Integration
- Template controls in collapsible section
- Doesn't interfere with existing inference functionality
- Clear separation between template UI and inference UI

### Accessibility
- Proper ARIA labels on buttons
- Semantic HTML structure
- Keyboard navigation support
- Clear visual indicators for state changes

## Default Templates

### 1. Code Review
- Variables: code, focus_areas
- Use: Review code for issues and improvements

### 2. Generate Documentation
- Variables: code_type, code, requirements
- Use: Auto-generate documentation

### 3. Bug Analysis
- Variables: description, code, error_message
- Use: Analyze bugs and propose solutions

### 4. Summarize Text
- Variables: length, focus, text
- Use: Create concise summaries

### 5. Explain Concept
- Variables: concept, audience, style
- Use: Explain concepts clearly

### 6. Creative Writing
- Variables: genre, topic, elements, tone
- Use: Generate creative content

## Best Practices

### For Template Creation
1. Use descriptive names and descriptions
2. Use consistent variable naming (snake_case)
3. Provide examples in description
4. Test variable substitution before saving
5. Organize by category for easy filtering

### For Template Usage
1. Review variable requirements before applying
2. Check preview before applying variables
3. Save frequently-used prompts as templates
4. Use categories to organize templates
5. Mark important templates as favorites

### For Integration
1. Template manager should be accessed via Templates button
2. Use getRecentTemplates() for quick access
3. Record usage via recordTemplateUsage()
4. Handle variable detection automatically
5. Provide visual feedback on template state

## Troubleshooting

### Templates Not Persisting
- Check browser localStorage is enabled
- Check browser console for storage quota errors
- Clear storage and recreate templates

### Variables Not Detected
- Ensure using `{{variable}}` syntax (double braces)
- Variable names must be alphanumeric
- Check for typos in variable names

### Substitution Not Working
- Verify variable names match exactly
- Check for extra spaces around variable names
- Ensure all variables have values

### Template Manager Won't Open
- Check if `showTemplateManager` state is updating
- Verify Dialog component is rendered
- Check console for errors

## Future Enhancements

Potential improvements:
- Server-side template storage/sharing
- Template versioning and history
- Team/shared template libraries
- Template tags and metadata
- Import/export templates
- Template usage analytics
- Variable validation rules
- Conditional template sections
