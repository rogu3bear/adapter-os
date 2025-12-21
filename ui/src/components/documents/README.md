# Document Components - Responsive Design Guide

## Quick Reference

### DocumentChatLayout

Responsive split-view layout for document chat with three layout modes:

```tsx
import DocumentChatLayout from '@/components/documents/DocumentChatLayout';

<DocumentChatLayout
  document={document}
  tenantId="tenant-id"
  collectionId="collection-id" // optional
  initialStackId="stack-id" // optional
/>
```

### Responsive Breakpoints

| Breakpoint | Width | Layout | PDF Access |
|------------|-------|--------|------------|
| Mobile | <768px | Chat only | Sheet drawer |
| Tablet | 768px-1024px | Collapsible split | Toggle button |
| Desktop | >1024px | Full split-view | Always visible |

### Features by Screen Size

#### Mobile (<768px)
- Chat interface takes full screen
- Floating "View PDF" button
- PDF opens in right-side drawer
- Auto-opens drawer when clicking evidence

#### Tablet (768px-1024px)
- Resizable split-view
- PDF panel can collapse/expand
- Toggle buttons for show/hide
- State persists to localStorage

#### Desktop (>1024px)
- Full resizable split-view
- Draggable resize handle
- Panel sizes persist to localStorage
- Optimized for mouse/trackpad

## Usage Examples

### Basic Usage
```tsx
import { useDocument } from '@/hooks/documents';
import DocumentChatLayout from '@/components/documents/DocumentChatLayout';

function MyPage() {
  const { data: document } = useDocument(documentId);
  
  if (!document) return <Loading />;
  
  return (
    <DocumentChatLayout
      document={document}
      tenantId={document.tenant_id}
    />
  );
}
```

### With Collection Context
```tsx
<DocumentChatLayout
  document={document}
  tenantId={document.tenant_id}
  collectionId="my-collection-123"
/>
```

### With Initial Stack
```tsx
<DocumentChatLayout
  document={document}
  tenantId={document.tenant_id}
  initialStackId="stack-xyz"
/>
```

## LocalStorage Keys

The component uses localStorage to persist user preferences:

- **`document-chat-panel-sizes`**: Panel size percentages `[chatWidth, pdfWidth]`
- **`document-chat-pdf-collapsed`**: Tablet collapse state `"true"` | `"false"`

## Customization

### Modify Default Panel Sizes
Edit the fallback in `DocumentChatLayout.tsx`:

```tsx
const [defaultSizes, setDefaultSizes] = useState<number[]>(() => {
  // Default: 50/50 split
  return stored ? JSON.parse(stored) : [50, 50]; // Change here
});
```

### Change Breakpoints
Breakpoints use Tailwind's default:
- `md:` = 768px
- `lg:` = 1024px

To customize, update Tailwind config or change the responsive classes.

### Modify Mobile Drawer Side
Change the drawer side in the Sheet component:

```tsx
<SheetContent side="right"> // Options: "left", "right", "top", "bottom"
```

## Accessibility

- All interactive elements have ARIA labels
- Sheet drawer has proper semantic structure
- Keyboard navigation preserved across layouts
- Focus management for panel transitions

## Testing Responsive Behavior

### Browser DevTools
1. Open browser DevTools (F12)
2. Toggle device toolbar (Ctrl+Shift+M / Cmd+Shift+M)
3. Test at different viewport widths:
   - 375px (mobile)
   - 768px (tablet minimum)
   - 1024px (desktop minimum)
   - 1440px (desktop large)

### Manual Testing Checklist
- [ ] Mobile: Floating button appears
- [ ] Mobile: Drawer opens/closes smoothly
- [ ] Mobile: Evidence items open drawer
- [ ] Tablet: Toggle buttons work
- [ ] Tablet: Collapse state persists
- [ ] Desktop: Panels resize smoothly
- [ ] Desktop: Panel sizes persist
- [ ] All: PDF loads correctly
- [ ] All: Evidence navigation works

## Common Issues

### PDF Not Loading
- Ensure document has `mime_type: 'application/pdf'`
- Check network tab for download errors
- Verify `downloadDocument` API is working

### Drawer Not Opening
- Check `mobileSheetOpen` state
- Verify Sheet component is rendered
- Ensure window width detection is working

### Panel Sizes Not Persisting
- Check localStorage is enabled
- Verify `STORAGE_KEY` is consistent
- Clear localStorage and test fresh

### Layout Breaks at Specific Width
- Check CSS class conflicts
- Verify Tailwind breakpoints
- Test with DevTools at exact breakpoint

## Performance Tips

1. **PDF URL Management**: Component creates blob URL once and cleans up on unmount
2. **LocalStorage**: Only read on component mount, not on every render
3. **Responsive Layouts**: Pure CSS, no JavaScript resize listeners
4. **Lazy Rendering**: Mobile drawer only renders when opened

## Related Components

- `PDFViewerEmbedded`: Embedded PDF viewer with navigation
- `ChatInterface`: Main chat component with evidence support
- `DocumentViewerContext`: Shared state for document navigation
- `useDocumentsApi`: API hooks for document operations

## Further Reading

- [RESPONSIVE_DOCUMENT_CHAT.md](/ui/RESPONSIVE_DOCUMENT_CHAT.md) - Full documentation
- [DocumentViewerContext.tsx](/ui/src/contexts/DocumentViewerContext.tsx) - Context API
- [PDFViewerEmbedded.tsx](/ui/src/components/documents/PDFViewerEmbedded.tsx) - PDF viewer
