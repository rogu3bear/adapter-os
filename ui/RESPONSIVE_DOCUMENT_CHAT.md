# Responsive Document Chat Interface

## Overview

The document chat interface now supports responsive design across all screen sizes with optimized layouts for mobile, tablet, and desktop devices.

## Responsive Breakpoints

### Mobile (<768px)
- **Layout**: Chat-only view
- **PDF Access**: Sheet drawer that slides in from the right
- **Features**:
  - Floating "View PDF" button in chat interface
  - Full-screen PDF viewer in drawer
  - Automatic drawer opening when clicking evidence items
  - Clean, touch-friendly interface

### Tablet (768px - 1024px)
- **Layout**: Collapsible split-view
- **PDF Panel**: Can be toggled on/off
- **Features**:
  - Collapsible PDF panel with toggle buttons
  - Persistent collapse state (localStorage)
  - "Show PDF" / "Hide PDF" toggle buttons
  - Resizable panels when both visible
  - Optimized for touch and mouse input

### Desktop (>1024px)
- **Layout**: Full resizable split-view
- **Features**:
  - Fully resizable panels with drag handle
  - Persistent panel sizes (localStorage)
  - Optimized for mouse/trackpad interaction
  - Maximum screen real estate utilization

## Implementation Details

### Files Updated

1. **`/ui/src/components/documents/DocumentChatLayout.tsx`**
   - Added responsive layout variations
   - Implemented mobile sheet drawer
   - Added collapsible panel logic for tablets
   - Maintained full split-view for desktop
   - Added localStorage persistence for panel states

2. **`/ui/src/pages/DocumentLibrary/DocumentChatPage.tsx`**
   - Updated header to be responsive
   - Optimized button labels for mobile
   - Added responsive spacing and text sizing
   - Integrated with new DocumentChatLayout

### Key Features

#### Mobile Sheet Drawer
```tsx
<Sheet open={mobileSheetOpen} onOpenChange={setMobileSheetOpen}>
  <SheetContent side="right" className="w-full p-0">
    <SheetHeader className="p-4 border-b">
      <SheetTitle className="flex items-center gap-2">
        <FileText className="h-5 w-5" />
        {document.name}
      </SheetTitle>
    </SheetHeader>
    <div className="h-[calc(100%-5rem)]">
      <PDFViewer />
    </div>
  </SheetContent>
</Sheet>
```

#### Collapsible Panel (Tablet)
```tsx
const [isPdfCollapsed, setIsPdfCollapsed] = useState(() => {
  try {
    const stored = localStorage.getItem(STORAGE_KEY_COLLAPSED);
    return stored === 'true';
  } catch {
    return false;
  }
});
```

#### Responsive Header
```tsx
<div className="p-2 sm:p-4 border-b flex items-center gap-2 sm:gap-4">
  <Button variant="ghost" size="sm" onClick={handleBack}>
    <ArrowLeft className="mr-2 h-4 w-4" />
    <span className="hidden sm:inline">Back</span>
  </Button>
  <div className="flex items-center gap-2 min-w-0 flex-1">
    <FileText className="h-4 w-4 sm:h-5 sm:w-5 text-muted-foreground" />
    <h1 className="text-base sm:text-lg font-semibold truncate">
      {document.name}
    </h1>
  </div>
</div>
```

### LocalStorage Keys

- `document-chat-panel-sizes`: Stores panel size percentages for desktop
- `document-chat-pdf-collapsed`: Stores collapsed state for tablet view

### Accessibility

- All toggle buttons have `aria-label` attributes
- Sheet drawer has proper ARIA structure
- Keyboard navigation preserved across all layouts
- Focus management for panel switching

### UX Enhancements

1. **Evidence Navigation**:
   - Mobile: Automatically opens PDF drawer when clicking evidence
   - Tablet/Desktop: Navigates to page in split-view

2. **State Persistence**:
   - Panel sizes saved per-user (localStorage)
   - Collapsed state remembered between sessions
   - Smooth transitions and animations

3. **Touch Optimization**:
   - Larger touch targets on mobile
   - Swipe-friendly drawer on mobile
   - Floating action button for easy access

## Testing Checklist

- [ ] Mobile (<768px): Chat works, PDF opens in drawer
- [ ] Mobile: Evidence items open PDF drawer automatically
- [ ] Mobile: Floating "View PDF" button visible and functional
- [ ] Tablet (768px-1024px): Panel collapses/expands correctly
- [ ] Tablet: Toggle buttons visible and functional
- [ ] Tablet: State persists across page reloads
- [ ] Desktop (>1024px): Full split-view with resizable panels
- [ ] Desktop: Panel sizes persist across page reloads
- [ ] All sizes: Header responsive and readable
- [ ] All sizes: Navigation works correctly
- [ ] All sizes: PDF loads and displays properly

## Browser Support

- Chrome/Edge: Full support
- Safari: Full support (iOS and macOS)
- Firefox: Full support
- Mobile browsers: Optimized for touch

## Future Enhancements

- [ ] Add keyboard shortcuts for panel toggling
- [ ] Add pan/zoom controls for mobile PDF viewer
- [ ] Implement swipe gestures for mobile drawer
- [ ] Add picture-in-picture mode for video documents
- [ ] Support for landscape/portrait orientation changes

## Visual Layout Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                         MOBILE (<768px)                             │
├─────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ ← Back              📄 Document.pdf                         │   │
│  └─────────────────────────────────────────────────────────────┘   │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                                                             │   │
│  │                  Chat Interface                             │   │
│  │                  (Full Width)                               │   │
│  │                                                             │   │
│  │                                                             │   │
│  │                                              ┌────────────┐ │   │
│  │                                              │ 📄 View PDF│ │   │
│  │                                              └────────────┘ │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  When "View PDF" clicked → Sheet drawer slides in from right →    │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ 📄 Document.pdf                                          ✕  │   │
│  ├─────────────────────────────────────────────────────────────┤   │
│  │                                                             │   │
│  │              PDF Viewer (Full Screen)                       │   │
│  │                                                             │   │
│  └─────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│                    TABLET (768px - 1024px)                          │
├─────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ ← Back              📄 Document.pdf          123 chunks     │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  Collapsed State:                                                   │
│  ┌────────────────────────────────────────────────┬──────────────┐ │
│  │                                         ☰      │   Show PDF   │ │
│  │          Chat Interface                        │              │ │
│  │          (Full Width)                          │              │ │
│  │                                                │              │ │
│  └────────────────────────────────────────────────┴──────────────┘ │
│                                                                     │
│  Expanded State:                                                    │
│  ┌──────────────────────────────────┬──────────────────────────┐   │
│  │                              ☰   │  ✕                       │   │
│  │     Chat Interface               │     PDF Viewer           │   │
│  │     (Resizable)                  │     (Resizable)          │   │
│  │                                  │                          │   │
│  └──────────────────────────────────┴──────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│                       DESKTOP (>1024px)                             │
├─────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ ← Back              📄 Document.pdf          123 chunks     │   │
│  └─────────────────────────────────────────────────────────────┘   │
│  ┌──────────────────────────────────┬──────────────────────────┐   │
│  │                                  │                          │   │
│  │     Chat Interface               │     PDF Viewer           │   │
│  │     (Resizable)                  │     (Resizable)          │   │
│  │                                  ║                          │   │
│  │                                  ║  ← Drag handle           │   │
│  │                                  ║                          │   │
│  │                                  │                          │   │
│  │                                  │                          │   │
│  └──────────────────────────────────┴──────────────────────────┘   │
│                                                                     │
│  Panel sizes persist to localStorage                                │
│  Fully resizable with drag handle (║)                              │
└─────────────────────────────────────────────────────────────────────┘
```

## Component Architecture

```
DocumentChatPage
├── Responsive Header
│   ├── Back button (← / ← Back)
│   ├── Document title (responsive sizing)
│   └── Chunk count (hidden on mobile)
│
└── DocumentChatLayout
    │
    ├── Mobile Layout (<768px)
    │   ├── ChatInterface (full width)
    │   ├── Floating "View PDF" button
    │   └── Sheet drawer
    │       └── PDFViewerEmbedded (full screen)
    │
    ├── Tablet Layout (768px-1024px)
    │   ├── ResizablePanelGroup
    │   │   ├── ChatInterface panel
    │   │   │   └── Collapse toggle button
    │   │   └── PDFViewerEmbedded panel (conditional)
    │   │       └── Close button
    │   └── "Show PDF" button (when collapsed)
    │
    └── Desktop Layout (>1024px)
        └── ResizablePanelGroup
            ├── ChatInterface panel
            ├── Resizable handle (drag)
            └── PDFViewerEmbedded panel
```

## State Management

### Component State
- `mobileSheetOpen`: Controls mobile drawer visibility
- `isPdfCollapsed`: Controls tablet panel collapse state
- `defaultSizes`: Panel size percentages for split-view

### LocalStorage Persistence
- `document-chat-panel-sizes`: `[50, 50]` (chat %, PDF %)
- `document-chat-pdf-collapsed`: `'true'` | `'false'`

### Auto-behaviors
- Mobile: Opens drawer when clicking evidence items
- Tablet: Remembers collapsed state across sessions
- Desktop: Remembers panel sizes across sessions

## CSS Classes Used

### Responsive Utilities
- `md:hidden` - Hide on tablet and larger
- `hidden md:flex` - Show on tablet only
- `hidden lg:block` - Show on desktop only
- `sm:inline` - Inline on small screens and larger
- `h-4 sm:h-5` - Responsive icon sizing
- `text-base sm:text-lg` - Responsive text sizing
- `p-2 sm:p-4` - Responsive padding

### Layout Classes
- `relative` - Position context for absolute children
- `absolute bottom-4 right-4` - Floating button position
- `z-10` - Ensure buttons appear above content
- `shadow-lg` - Floating button elevation
- `w-full` - Full width mobile drawer

## Integration Points

### ChatInterface
- Receives `documentContext` prop with document ID
- Calls `onViewDocument` when evidence is clicked
- Full-width on mobile, resizable on larger screens

### PDFViewerEmbedded
- Receives `ref` for programmatic control
- Used consistently across all layouts
- Supports `goToPage` and `scrollToText` methods

### Evidence Navigation Flow
1. User clicks evidence in chat
2. `onViewDocument` callback triggered
3. Mobile: Opens sheet drawer
4. Tablet/Desktop: Navigates within split-view
5. PDF viewer scrolls to highlighted text

## Performance Considerations

- PDF URL created once and cleaned up on unmount
- LocalStorage reads minimized (only on mount)
- Responsive layouts use CSS only (no JS resize listeners)
- Sheet drawer lazy-renders (only when opened)
- Panel size changes debounced by ResizablePanelGroup
