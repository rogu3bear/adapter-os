# AdapterOS Design Tokens Reference

**Last Updated**: 2025-01-15
**Location**: `ui/src/styles/design-system.css`
**Purpose**: Comprehensive reference for all design tokens in the AdapterOS UI

---

## Table of Contents

- [Color Tokens](#color-tokens)
- [Typography Tokens](#typography-tokens)
- [Spacing & Layout](#spacing--layout)
- [Radii & Borders](#radii--borders)
- [Shadows](#shadows)
- [Animation & Transitions](#animation--transitions)
- [Component Tokens](#component-tokens)
- [Utility Functions](#utility-functions)
- [Best Practices](#best-practices)

---

## Color Tokens

### Semantic Colors (HSL Format)

Use semantic colors for UI elements to ensure proper theming and accessibility.

```css
--background: 0 0% 100%           /* Main background */
--foreground: 222.2 84% 4.9%      /* Main text color */
--card: 0 0% 100%                 /* Card background */
--card-foreground: 222.2 84% 4.9% /* Card text */
--popover: 0 0% 100%              /* Popover background */
--primary: 222.2 47.4% 11.2%      /* Primary brand color */
--secondary: 210 40% 96%          /* Secondary color */
--muted: 210 40% 96%              /* Muted background */
--muted-foreground: 215.4 16.3% 46.9% /* Muted text */
--accent: 210 40% 96%             /* Accent color */
--destructive: 0 84.2% 60.2%      /* Error/danger color */
--border: 214.3 31.8% 91.4%       /* Border color */
--input: 214.3 31.8% 91.4%        /* Input border */
--ring: 222.2 84% 4.9%            /* Focus ring color */
```

**Usage**:
```tsx
// In Tailwind classes
className="bg-background text-foreground"
className="bg-muted text-muted-foreground"

// In inline styles
style={{ color: 'hsl(var(--primary))' }}
```

### Status Colors (HSL Format)

```css
--error: 0 84.2% 60.2%             /* Error state */
--error-surface: var(--surface-2)  /* Error background */
--error-border: 0 70% 60%          /* Error border */

--success: 142.1 70.6% 45.3%       /* Success state */
--success-surface: var(--surface-2)/* Success background */
--success-border: 142.1 50% 70%    /* Success border */

--warning: 43.9 96.1% 72.2%        /* Warning state */
--warning-surface: var(--surface-2)/* Warning background */
--warning-border: 43.9 80% 60%     /* Warning border */

--info: 221.2 83.2% 53.3%          /* Info state */
--info-surface: var(--surface-2)   /* Info background */
--info-border: 221.2 70% 60%       /* Info border */
```

**Usage with Badge Component**:
```tsx
<Badge variant="error">Error</Badge>
<Badge variant="success">Success</Badge>
<Badge variant="warning">Warning</Badge>
<Badge variant="info">Info</Badge>
```

### Chart Colors (HSL Format)

Use for data visualization to ensure consistency.

```css
--chart-1: 12 76% 61%    /* Primary chart color - orange/coral */
--chart-2: 173 58% 39%   /* Secondary - teal */
--chart-3: 197 37% 24%   /* Tertiary - dark blue */
--chart-4: 43 74% 66%    /* Quaternary - yellow */
--chart-5: 27 87% 67%    /* Quinary - orange */
```

**Usage with Utility Functions**:
```tsx
import { getChartColor } from '@/components/ui/utils';

// In chart components
<Line stroke={getChartColor(1)} />
<Area fill={getChartColor(2)} fillOpacity={0.6} />
```

### Gray Scale (RGB Format for Tailwind)

```css
--gray-50: 249 250 251    /* Lightest gray */
--gray-100: 243 244 246
--gray-200: 229 231 235
--gray-300: 209 213 219
--gray-400: 156 163 175
--gray-500: 107 114 128
--gray-600: 75 85 99
--gray-700: 55 65 81
--gray-800: 31 41 55
--gray-900: 17 24 39      /* Darkest gray */
--gray-950: 3 7 18
```

**⚠️ Important**: Use semantic tokens (`--muted-foreground`, `--border`) instead of direct gray references in most cases.

### Surface Tokens

```css
--surface-1: 0 0% 98%     /* Main surface */
--surface-2: 0 0% 95%     /* Elevated surface */
--surface-3: 0 0% 92%     /* More elevated surface */
```

---

## Typography Tokens

### Font Weights

```css
--font-weight-regular: 400    /* Body text */
--font-weight-medium: 500     /* Subheadings */
--font-weight-semibold: 600   /* Headings */
--font-weight-bold: 700       /* Emphasis */
```

### Font Sizes (Fluid Typography)

Uses `clamp()` for responsive scaling.

```css
--font-display: clamp(3rem, 5vw + 1.25rem, 4rem)     /* 48–64px */
--font-h1: clamp(2rem, 3vw + 1rem, 2.5rem)           /* 32–40px */
--font-h2: clamp(1.5rem, 2vw + 0.875rem, 1.75rem)    /* 24–28px */
--font-h3: clamp(1.25rem, 1.2vw + 0.95rem, 1.5rem)   /* 20–24px */
--font-body: clamp(1rem, 0.5vw + 0.75rem, 1.125rem)  /* 16–18px */
--font-caption: clamp(0.75rem, 0.25vw + 0.65rem, 0.875rem) /* 12–14px */
```

### Line Heights

```css
--line-height-tight: 1.1      /* Display text */
--line-height-snug: 1.3       /* Headings */
--line-height-body: 1.5       /* Body text */
--line-height-relaxed: 1.6    /* Long-form content */
```

**Usage**:
```tsx
<h1 style={{
  fontSize: 'var(--font-h1)',
  lineHeight: 'var(--line-height-tight)',
  fontWeight: 'var(--font-weight-bold)'
}}>
  Page Title
</h1>
```

---

## Spacing & Layout

### Base Unit System

```css
--base-unit: 4px              /* Foundation for all spacing */
--space-1: calc(var(--base-unit) * 1)   /* 4px */
--space-2: calc(var(--base-unit) * 2)   /* 8px */
--space-3: calc(var(--base-unit) * 3)   /* 12px */
--space-4: calc(var(--base-unit) * 4)   /* 16px */
--space-5: calc(var(--base-unit) * 5)   /* 20px */
--space-6: calc(var(--base-unit) * 6)   /* 24px */
--space-8: calc(var(--base-unit) * 8)   /* 32px */
--space-10: calc(var(--base-unit) * 10) /* 40px */
--space-12: calc(var(--base-unit) * 12) /* 48px */
--space-16: calc(var(--base-unit) * 16) /* 64px */
```

### Breakpoints

```css
--bp-sm: 30rem   /* 480px  - Mobile landscape */
--bp-md: 48rem   /* 768px  - Tablet */
--bp-lg: 64rem   /* 1024px - Desktop */
--bp-xl: 90rem   /* 1440px - Large desktop */
```

---

## Radii & Borders

### Border Radii

```css
--radius-1: calc(var(--base-unit) * 1)  /* 4px  - Tight */
--radius-2: calc(var(--base-unit) * 2)  /* 8px  - Small */
--radius-3: calc(var(--base-unit) * 3)  /* 12px - Medium */
--radius-4: calc(var(--base-unit) * 4)  /* 16px - Large */
--radius-5: calc(var(--base-unit) * 6)  /* 24px - Extra large */
```

### Component Radii (Semantic)

```css
--radius-button: var(--radius-2)   /* Button corners */
--radius-card: var(--radius-3)     /* Card corners */
--radius-input: var(--radius-2)    /* Input field corners */
--radius-surface: var(--radius-4)  /* Large surface corners */
```

---

## Shadows

```css
--shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.08)      /* Subtle elevation */
--shadow-md: 0 4px 12px rgba(0, 0, 0, 0.12)     /* Standard elevation */
--shadow-lg: 0 12px 24px rgba(0, 0, 0, 0.16)    /* High elevation */
```

**Usage**:
```tsx
<div style={{ boxShadow: 'var(--shadow-md)' }}>
  Elevated card
</div>
```

---

## Animation & Transitions

### Duration

```css
--transition-fast: 0.1s    /* Quick feedback */
--transition-base: 0.15s   /* Standard transitions */
--transition-slow: 0.3s    /* Deliberate motion */
```

### Timing Function

```css
--transition-timing: cubic-bezier(0.4, 0, 0.2, 1)  /* Smooth easing */
```

**Usage**:
```css
.button {
  transition: background-color var(--transition-base) var(--transition-timing);
}
```

### Accessibility: Reduced Motion

```css
@media (prefers-reduced-motion: reduce) {
  * {
    animation-duration: 0.01ms !important;
    transition-duration: 0.01ms !important;
  }
}
```

---

## Component Tokens

### Buttons

```css
--button-padding-y: var(--space-2)   /* Vertical padding */
--button-padding-x: var(--space-3)   /* Horizontal padding */
--button-radius: var(--radius-button) /* Border radius */
```

### Cards

```css
--card-shadow: var(--shadow-md)    /* Card elevation */
--card-radius: var(--radius-card)  /* Card corners */
--card-gap: var(--space-4)         /* Internal spacing */
```

### Inputs

```css
--input-padding-y: var(--space-2)      /* Vertical padding */
--input-padding-x: var(--space-3)      /* Horizontal padding */
--input-border-width: 1px              /* Border thickness */
```

### Navbar

```css
--navbar-height: calc(var(--base-unit) * 10)  /* 40px - Fixed height */
--navbar-padding-x: var(--space-4)            /* Horizontal padding */
```

---

## Utility Functions

### Color Conversion Functions

Located in `ui/src/components/ui/utils.ts`:

#### `hslToHex(hslString: string): string`

Converts HSL CSS variable values to hex colors for chart libraries.

```tsx
import { hslToHex } from '@/components/ui/utils';

const hexColor = hslToHex('12 76% 61%');
// Returns: "#f87171"
```

#### `getChartColor(index: number): string`

Gets chart color from CSS variable by index (1-5).

```tsx
import { getChartColor } from '@/components/ui/utils';

const chartColor1 = getChartColor(1);  // Returns hex color from --chart-1
const chartColor2 = getChartColor(2);  // Returns hex color from --chart-2
```

#### `getSemanticColor(colorName: string): string`

Gets semantic color from CSS variable.

```tsx
import { getSemanticColor } from '@/components/ui/utils';

const borderColor = getSemanticColor('border');
const successColor = getSemanticColor('success');
```

---

## Best Practices

### ✅ DO

1. **Use Semantic Tokens**
   ```tsx
   // Good
   <div className="bg-background text-foreground border-border">
   ```

2. **Use Chart Color Functions**
   ```tsx
   // Good
   import { getChartColor } from '@/components/ui/utils';
   <Line stroke={getChartColor(1)} />
   ```

3. **Use Tailwind Classes with Tokens**
   ```tsx
   // Good
   <div className="bg-muted text-muted-foreground">
   ```

4. **Use Status Variants**
   ```tsx
   // Good
   <Badge variant="success">Active</Badge>
   <Alert variant="destructive">Error</Alert>
   ```

### ❌ DON'T

1. **Don't Use Hardcoded Colors**
   ```tsx
   // Bad
   <div style={{ color: '#8884d8' }}>

   // Bad
   <div className="text-gray-400">
   ```

2. **Don't Use Direct Tailwind Color Classes**
   ```tsx
   // Bad
   <div className="bg-blue-500">

   // Good
   <div className="bg-primary">
   ```

3. **Don't Bypass Design Tokens**
   ```tsx
   // Bad
   <Line stroke="#2563eb" />

   // Good
   <Line stroke={getChartColor(1)} />
   ```

### When to Use Inline Styles vs Classes

**Use Tailwind Classes** (preferred):
```tsx
<div className="bg-muted text-muted-foreground">
```

**Use Inline Styles** when:
- Dynamic colors needed
- Chart libraries require hex values
- CSS variable access needed

```tsx
<Star style={{ color: `hsl(var(--warning))` }} />
```

---

## Dark Mode Support

Design tokens automatically adapt to dark mode via `@media (prefers-color-scheme: dark)`.

### Dark Mode Overrides

```css
@media (prefers-color-scheme: dark) {
  :root {
    --surface-1: 222.2 47.4% 11.2%;   /* Dark background */
    --surface-2: 222.2 47.4% 15%;     /* Elevated dark */
    --surface-3: 222.2 47.4% 18%;     /* More elevated dark */
    --error: 0 62.8% 50.6%;           /* Adjusted for dark mode */
  }
}
```

**No code changes needed** - tokens automatically switch.

---

## Common Patterns

### Card with Proper Tokens

```tsx
<Card className="bg-card border-border">
  <CardHeader>
    <CardTitle className="text-card-foreground">Title</CardTitle>
  </CardHeader>
  <CardContent className="text-muted-foreground">
    Content
  </CardContent>
</Card>
```

### Button with Proper Variants

```tsx
<Button variant="default">Primary Action</Button>
<Button variant="secondary">Secondary Action</Button>
<Button variant="destructive">Delete</Button>
<Button variant="outline">Cancel</Button>
<Button variant="ghost">Subtle Action</Button>
```

### Chart with Design Tokens

```tsx
import { getChartColor } from '@/components/ui/utils';

<ResponsiveContainer>
  <LineChart data={data}>
    <Line
      dataKey="value"
      stroke={getChartColor(1)}
      strokeWidth={2}
    />
    <Line
      dataKey="target"
      stroke={getChartColor(2)}
      strokeWidth={2}
    />
  </LineChart>
</ResponsiveContainer>
```

---

## Migration Guide

### Migrating Hardcoded Colors

**Before**:
```tsx
<div className="text-gray-500 bg-gray-100 border-gray-300">
  <Star className="text-yellow-400" />
</div>
```

**After**:
```tsx
<div className="text-muted-foreground bg-muted border-border">
  <Star style={{ color: `hsl(var(--warning))` }} />
</div>
```

### Migrating Chart Colors

**Before**:
```tsx
<Line stroke="#8884d8" />
<Area fill="#10b981" />
```

**After**:
```tsx
import { getChartColor } from '@/components/ui/utils';

<Line stroke={getChartColor(1)} />
<Area fill={getChartColor(2)} />
```

---

## Testing & Verification

### Color Contrast Verification

Ensure WCAG AA compliance:
- **Normal text**: 4.5:1 contrast ratio
- **Large text**: 3:1 contrast ratio
- **UI components**: 3:1 contrast ratio

Use browser DevTools or online tools to verify:
- [WebAIM Contrast Checker](https://webaim.org/resources/contrastchecker/)
- Chrome DevTools > Lighthouse > Accessibility

### Visual Regression Testing

Test token changes across:
- Light mode
- Dark mode
- All component variants
- Different viewport sizes

---

## References

- **CSS File**: `/ui/src/styles/design-system.css`
- **Utility Functions**: `/ui/src/components/ui/utils.ts`
- **Component Examples**: `/ui/src/components/ui/`
- **Tailwind Config**: `/ui/tailwind.config.ts`

---

**Maintained by**: AdapterOS UI Team
**Questions**: See `AGENTS.md` for contribution guidelines
