# Responsive Design Guide

**Purpose:** Developer guide for implementing responsive layouts in AdapterOS UI
**Last Updated:** 2025-01-15

---

## Table of Contents

- [Breakpoints](#breakpoints)
- [Responsive Patterns](#responsive-patterns)
- [Common Components](#common-components)
- [Testing](#testing)
- [Best Practices](#best-practices)

---

## Breakpoints

AdapterOS uses Tailwind CSS with the following breakpoints:

```css
/* From ui/src/styles/design-system.css */
--bp-sm: 30rem   /* 480px  - Mobile landscape */
--bp-md: 48rem   /* 768px  - Tablet */
--bp-lg: 64rem   /* 1024px - Desktop */
--bp-xl: 90rem   /* 1440px - Large desktop */
```

### Tailwind Breakpoint Classes

```tsx
// Mobile-first approach (default styles apply to mobile)
sm:  // ≥640px  (mobile landscape & up)
md:  // ≥768px  (tablet & up)
lg:  // ≥1024px (desktop & up)
xl:  // ≥1280px (large desktop & up)
```

---

## Responsive Patterns

### 1. Grid Layouts

**Always include tablet breakpoint** for better 768px-1024px experience:

```tsx
// ✅ GOOD - Progressive grid with tablet support
<div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4">
  {items.map(item => <Card key={item.id}>{item.content}</Card>)}
</div>

// ❌ BAD - Jumps from 1 to 3 columns, poor tablet experience
<div className="grid grid-cols-1 md:grid-cols-3 gap-4">
```

**Common Grid Patterns:**

```tsx
// 2-column layout
<div className="grid grid-cols-1 sm:grid-cols-2 gap-6">

// 3-column layout
<div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-4">

// 4-column layout
<div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4">

// Dashboard widgets
<div className="grid grid-cols-1 sm:grid-cols-2 gap-6">
```

### 2. Table Responsiveness

**Pattern A: Horizontal Scroll** (for complex tables)

```tsx
<CardContent className="px-0 sm:px-6">
  <div className="overflow-x-auto">
    <div className="min-w-[800px]">
      <Table>
        {/* Table content */}
      </Table>
    </div>
  </div>
</CardContent>
```

**Pattern B: Column Hiding** (for simpler tables)

```tsx
<TableHeader>
  <TableRow>
    <TableHead>Name</TableHead> {/* Always visible */}
    <TableHead className="hidden sm:table-cell">Category</TableHead>
    <TableHead>State</TableHead> {/* Always visible */}
    <TableHead className="hidden md:table-cell">Memory</TableHead>
    <TableHead className="hidden lg:table-cell">Activations</TableHead>
    <TableHead className="hidden lg:table-cell">Last Used</TableHead>
    <TableHead>Actions</TableHead> {/* Always visible */}
  </TableRow>
</TableHeader>

<TableBody>
  <TableRow>
    <TableCell>{adapter.name}</TableCell>
    <TableCell className="hidden sm:table-cell">{adapter.category}</TableCell>
    <TableCell>{adapter.state}</TableCell>
    <TableCell className="hidden md:table-cell">{adapter.memory}</TableCell>
    <TableCell className="hidden lg:table-cell">{adapter.activations}</TableCell>
    <TableCell className="hidden lg:table-cell">{adapter.lastUsed}</TableCell>
    <TableCell>{actions}</TableCell>
  </TableRow>
</TableBody>
```

**Column Hiding Strategy:**
- **Keep visible:** Primary identifier (Name), Status/State, Actions
- **Hide on mobile** (`hidden sm:table-cell`): Category, Secondary info
- **Hide on tablet** (`hidden md:table-cell`): Memory, Performance metrics
- **Hide on desktop** (`hidden lg:table-cell`): Timestamps, Counts

### 3. Dialog/Modal Sizing

**Always use responsive max-widths:**

```tsx
// ✅ GOOD - Responsive with viewport margin
<DialogContent className="max-w-[calc(100%-2rem)] sm:max-w-lg">

<DialogContent className="max-w-[calc(100%-2rem)] sm:max-w-2xl">

<DialogContent className="max-w-[calc(100%-2rem)] sm:max-w-2xl md:max-w-3xl lg:max-w-4xl">

// ❌ BAD - Fixed size, no mobile adaptation
<DialogContent className="max-w-4xl">
```

**Dialog Size Guidelines:**

- **Small** (forms, confirmations): `sm:max-w-md` or `sm:max-w-lg`
- **Medium** (detailed views): `sm:max-w-2xl`
- **Large** (wizards, complex UIs): `sm:max-w-2xl md:max-w-3xl lg:max-w-4xl`
- **Extra Large** (full features): `sm:max-w-2xl md:max-w-4xl lg:max-w-5xl xl:max-w-6xl`

**Height Management:**

```tsx
// Mobile-friendly heights
max-h-[80vh]  // Short content
max-h-[85vh]  // Standard
max-h-[90vh]  // Maximum (avoid on mobile)

// Always include overflow
<DialogContent className="... max-h-[85vh] overflow-y-auto">
```

### 4. Flex Direction

```tsx
// ✅ GOOD - Stack on mobile, row on desktop
<div className="flex flex-col md:flex-row items-start md:items-center gap-4">
  <h1>Title</h1>
  <div className="flex gap-2">
    <Button>Action 1</Button>
    <Button>Action 2</Button>
  </div>
</div>

// Card layouts
<Card className="flex flex-col md:flex-row">
  <div>Left content</div>
  <div>Right content</div>
</Card>
```

### 5. Visibility Control

```tsx
// Hide on mobile, show on desktop
<div className="hidden md:block">Desktop only content</div>

// Show on mobile, hide on desktop
<div className="md:hidden">Mobile only content</div>

// Inline vs block display
<span className="hidden sm:inline">Full label</span>
<Badge className="hidden sm:inline-flex">Badge</Badge>

// Table cells
<TableHead className="hidden md:table-cell">Column</TableHead>
<TableCell className="hidden md:table-cell">Data</TableCell>
```

### 6. Spacing & Padding

```tsx
// Responsive padding
<div className="p-4 md:p-6">
<div className="px-4 py-6 sm:p-6">

// Responsive gaps
<div className="gap-4 sm:gap-6">
<div className="space-y-4 md:space-y-6">

// Remove padding on mobile for full-width tables
<CardContent className="px-0 sm:px-6">
```

### 7. Typography

Use fluid typography from design tokens:

```tsx
// Font sizes automatically scale (clamp in CSS)
<h1 className="[font-size:var(--font-h1)]">Title</h1>
<h2 className="[font-size:var(--font-h2)]">Subtitle</h2>
<p className="[font-size:var(--font-body)]">Body text</p>

// Or use Tailwind responsive classes
<h1 className="text-2xl md:text-3xl lg:text-4xl">Title</h1>
```

---

## Common Components

### Responsive Card

```tsx
<Card>
  <CardHeader className="flex-col sm:flex-row sm:items-center sm:justify-between">
    <CardTitle>Title</CardTitle>
    <div className="flex gap-2 mt-2 sm:mt-0">
      <Button size="sm">Action</Button>
    </div>
  </CardHeader>
  <CardContent className="px-0 sm:px-6">
    {/* Content */}
  </CardContent>
</Card>
```

### Responsive Navigation

```tsx
// Mobile: Hamburger menu
<Button className="md:hidden" onClick={toggleSidebar}>
  <Menu />
</Button>

// Desktop: Full sidebar
<aside className="hidden md:block w-64">
  {/* Sidebar content */}
</aside>

// Mobile: Overlay sidebar
<aside className={`fixed inset-y-0 left-0 z-50 w-64 transform ${
  isOpen ? 'translate-x-0' : '-translate-x-full'
} transition-transform md:relative md:translate-x-0`}>
  {/* Sidebar content */}
</aside>
```

### Responsive Tabs

```tsx
<TabsList className="grid w-full grid-cols-4">
  <TabsTrigger value="tab1" aria-label="Tab 1">
    <Icon className="h-4 w-4" />
    <span className="hidden sm:inline">Tab 1</span>
  </TabsTrigger>
  {/* More tabs */}
</TabsList>
```

---

## Testing

### Breakpoints to Test

1. **375px** - iPhone SE, small phones
2. **640px** - Mobile landscape, phablets
3. **768px** - iPad Portrait, tablets
4. **1024px** - iPad Landscape, small laptops
5. **1440px** - Desktop

### Testing Checklist

- [ ] Tables scroll horizontally or hide columns appropriately
- [ ] Modals fit within viewport with margins
- [ ] Navigation is accessible (hamburger menu works)
- [ ] Touch targets are minimum 44x44px
- [ ] Charts resize within containers
- [ ] Forms are completable without horizontal scroll
- [ ] Action buttons are tappable on mobile
- [ ] Text is readable (not too small)
- [ ] Grids reflow correctly at each breakpoint
- [ ] No content overflow or cut-off

### Browser DevTools

```bash
# Chrome DevTools
1. Open DevTools (F12)
2. Click device toolbar icon
3. Test at different viewport sizes
4. Use "Responsive" mode for custom sizes

# Test orientations
- Portrait (default)
- Landscape (rotate in DevTools)
```

---

## Best Practices

### ✅ DO

1. **Use Mobile-First Approach**
   ```tsx
   // Default styles for mobile, enhance for larger screens
   <div className="flex-col md:flex-row">
   ```

2. **Progressive Enhancement**
   ```tsx
   // Add features as screen size increases
   <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3">
   ```

3. **Touch-Friendly Targets**
   ```tsx
   // Minimum 44px touch targets (WCAG 2.1)
   <Button className="min-h-[44px]">
   ```

4. **Consistent Patterns**
   ```tsx
   // Use established patterns from this guide
   // Don't invent new responsive patterns without good reason
   ```

5. **Test on Real Devices**
   ```bash
   # Not just browser DevTools
   - Test on actual phones/tablets
   - Test touch interactions
   - Test different screen densities
   ```

### ❌ DON'T

1. **Don't Skip Tablet Breakpoint**
   ```tsx
   // ❌ BAD
   <div className="grid-cols-1 md:grid-cols-4">

   // ✅ GOOD
   <div className="grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4">
   ```

2. **Don't Use Fixed Widths**
   ```tsx
   // ❌ BAD
   <div className="w-[800px]">

   // ✅ GOOD
   <div className="w-full max-w-4xl">
   ```

3. **Don't Forget Viewport Margins**
   ```tsx
   // ❌ BAD
   <DialogContent className="max-w-6xl">

   // ✅ GOOD
   <DialogContent className="max-w-[calc(100%-2rem)] sm:max-w-6xl">
   ```

4. **Don't Hide Critical Actions**
   ```tsx
   // ❌ BAD - Primary action hidden on mobile
   <Button className="hidden md:block">Submit</Button>

   // ✅ GOOD - Always visible
   <Button>Submit</Button>
   ```

5. **Don't Rely Only on Hover**
   ```tsx
   // ❌ BAD - Mobile has no hover
   .button:hover { /* critical info */ }

   // ✅ GOOD - Works on touch
   .button:hover, .button:focus { /* info */ }
   ```

---

## Migration Checklist

When adding responsive design to existing components:

- [ ] Add `sm:` breakpoint to grids that jump from 1 to 2+ columns
- [ ] Wrap wide tables in `overflow-x-auto` container
- [ ] Add column hiding to tables with 6+ columns
- [ ] Update dialog max-widths with responsive breakpoints
- [ ] Add `aria-label` to icons when text is hidden on mobile
- [ ] Test at 375px, 768px, and 1024px breakpoints
- [ ] Verify touch targets are 44x44px minimum
- [ ] Check that all actions are accessible on mobile

---

## Examples from Codebase

### Good Examples

**Dashboard.tsx** (Line 255)
```tsx
<div className="grid grid-cols-1 sm:grid-cols-2 gap-6">
```

**Adapters.tsx** (Lines 1369-1371)
```tsx
<CardContent className="px-0 sm:px-6">
  <div className="overflow-x-auto">
    <div className="max-h-[600px] overflow-y-auto min-w-[800px]">
```

**RootLayout.tsx** (Line 191)
```tsx
<aside className={`fixed ... ${isSidebarOpen ? 'translate-x-0' : '-translate-x-full'}
  transition-transform md:relative md:translate-x-0 ...`}>
```

**Adapters.tsx Table Columns** (Lines 1396-1400)
```tsx
<TableHead className="... hidden sm:table-cell">Category</TableHead>
<TableHead className="...">State</TableHead>
<TableHead className="... hidden md:table-cell">Memory</TableHead>
<TableHead className="... hidden lg:table-cell">Activations</TableHead>
```

---

## Resources

- **Design Tokens:** `ui/docs/design-tokens.md`
- **Tailwind Docs:** https://tailwindcss.com/docs/responsive-design
- **WCAG Touch Targets:** https://www.w3.org/WAI/WCAG21/Understanding/target-size.html
- **Testing Tools:** Chrome DevTools, Firefox Responsive Design Mode

---

**Maintained by:** AdapterOS UI Team
**Questions:** See `AGENTS.md` for contribution guidelines
