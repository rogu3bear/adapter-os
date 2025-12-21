# Collection Management UI Integration Points

## Files Created

### 1. **Core Components**
- `/Users/mln-dev/Dev/adapter-os/ui/src/components/collections/CollectionManager.tsx`
  - Full-featured collection management interface
  - Create, list, select, and delete collections
  - Displays document count per collection
  - Visual selection state with border highlighting

- `/Users/mln-dev/Dev/adapter-os/ui/src/components/collections/CollectionSelector.tsx`
  - Lightweight dropdown selector for collections
  - Shows collection name and document count
  - Used in simplified flows (e.g., chat interface, training wizard)

- `/Users/mln-dev/Dev/adapter-os/ui/src/components/collections/AddDocumentsDialog.tsx`
  - Dialog to add existing documents to a collection
  - Checkbox-based multi-select interface
  - Search/filter functionality for document list
  - Shows document metadata (type, size)

- `/Users/mln-dev/Dev/adapter-os/ui/src/components/collections/index.ts`
  - Barrel export for all collection components

### 2. **Constants**
- `/Users/mln-dev/Dev/adapter-os/ui/src/constants/terminology.ts`
  - Centralized terminology definitions
  - Ensures consistent UI text across components
  - Key terms: `datasetName`, `datasetDescription`, `selectDataset`

## Integration Points in TrainingWizard.tsx

### Simple Mode: Dataset Selection (Lines 437-523)
**Current Implementation:**
```typescript
// Line 463-500: Direct Select component with datasets array
<Select value={state.datasetId} onValueChange={(value) => { ... }}>
  <SelectTrigger id="dataset">
    <SelectValue placeholder="Choose a dataset..." />
  </SelectTrigger>
  <SelectContent>
    {datasets.map((dataset) => (
      <SelectItem key={dataset.id} value={dataset.id}>
        <div className="flex items-center gap-2">
          <span>{dataset.name}</span>
          <Badge variant="outline">{dataset.validation_status}</Badge>
        </div>
      </SelectItem>
    ))}
  </SelectContent>
</Select>
```

**Recommended Integration:**
```typescript
import { CollectionSelector } from '@/components/collections';

// Replace lines 463-500 with:
<CollectionSelector
  collections={datasets.map(d => ({
    id: d.id,
    name: d.name,
    document_count: d.document_count || 0
  }))}
  selectedId={state.datasetId || null}
  onSelect={(id) => {
    updateState({
      datasetId: id,
      dataSourceType: 'dataset',
      // ... existing defaults
    });
  }}
  placeholder="Choose a collection..."
/>
```

**Benefits:**
- Consistent UI/UX with collection terminology
- Reduced code duplication
- Automatic document count display
- Centralized styling and behavior

### Advanced Mode: Data Source Step (Lines 857-903)
**Current Implementation:**
```typescript
// Line 860-880: Similar Select component structure
<Select value={state.datasetId} onValueChange={(value) => updateState({ datasetId: value })}>
  {/* ... similar structure to simple mode */}
</Select>
```

**Recommended Integration:**
```typescript
import { CollectionSelector } from '@/components/collections';

// Replace lines 860-880 with:
<CollectionSelector
  collections={datasets.map(d => ({
    id: d.id,
    name: d.name,
    document_count: d.document_count || 0
  }))}
  selectedId={state.datasetId || null}
  onSelect={(id) => updateState({ datasetId: id })}
  placeholder="Choose a collection..."
/>
```

## Additional Integration Opportunities

### 1. **Chat Interface Collection Selection**
When implementing chat context with document collections:

```typescript
import { CollectionSelector } from '@/components/collections';

// In chat settings/context panel:
<CollectionSelector
  collections={availableCollections}
  selectedId={currentCollectionId}
  onSelect={handleCollectionChange}
  placeholder="Select document collection for context"
/>
```

### 2. **Training Datasets Page**
Replace dataset terminology with collection terminology:

```typescript
import { CollectionManager } from '@/components/collections';

// In DatasetsTab.tsx or similar:
<CollectionManager
  onSelectCollection={(collection) => {
    // Navigate to collection detail view
    navigate(`/training/collections/${collection.id}`);
  }}
  selectedCollectionId={selectedId}
/>
```

### 3. **Document Management**
For adding documents to collections:

```typescript
import { AddDocumentsDialog } from '@/components/collections';

// In document list view:
<AddDocumentsDialog
  collectionId={currentCollectionId}
  onDocumentsAdded={() => {
    // Refresh collection document list
    fetchCollectionDocuments();
  }}
/>
```

## API Endpoints Expected

The collection components expect these backend endpoints:

1. **GET /v1/collections**
   - Returns: `Array<{ id, name, description, document_count, created_at }>`

2. **POST /v1/collections**
   - Body: `{ name, description? }`
   - Returns: Collection object

3. **DELETE /v1/collections/:id**
   - Returns: 204 No Content

4. **GET /v1/collections/:id/available-documents**
   - Returns: `Array<{ id, name, type, size, created_at }>`
   - Documents not already in this collection

5. **POST /v1/collections/:id/documents**
   - Body: `{ document_ids: string[] }`
   - Returns: 201 Created

## Migration Strategy

### Phase 1: Non-Breaking Integration
1. Keep existing dataset selection in TrainingWizard
2. Add CollectionSelector as alternative in chat/inference contexts
3. Add CollectionManager to new "Collections" page

### Phase 2: Terminology Unification
1. Update TrainingWizard to use CollectionSelector
2. Rename "Datasets" tab to "Collections" tab
3. Update all UI text to use `TERMS` constants

### Phase 3: Full Collection Model
1. Migrate backend dataset model to collection model
2. Update all API clients to use collection endpoints
3. Remove deprecated dataset terminology

## Testing Checklist

- [ ] CollectionManager creates collections successfully
- [ ] CollectionSelector displays collections with document counts
- [ ] AddDocumentsDialog filters and adds documents correctly
- [ ] TrainingWizard integrates with CollectionSelector (simple mode)
- [ ] TrainingWizard integrates with CollectionSelector (advanced mode)
- [ ] Terminology constants used consistently across UI
- [ ] Backend API endpoints respond correctly
- [ ] Collection deletion confirms before removing
- [ ] Search/filter in AddDocumentsDialog works
- [ ] Document count updates after adding/removing documents

## Next Steps

1. **Immediate:** Test components in isolation with mock data
2. **Backend:** Implement collection API endpoints (if not already present)
3. **Integration:** Replace dataset selectors in TrainingWizard with CollectionSelector
4. **UI Polish:** Add loading states, error handling, empty states
5. **Documentation:** Update user-facing docs to use "collection" terminology
