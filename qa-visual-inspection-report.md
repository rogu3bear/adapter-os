# Visual Inspection Report - adapterOS UI

**Date:** January 12, 2026  
**System:** adapterOS UI (Leptos WASM)  
**Base URL:** http://localhost:8080  
**Browser:** Open in default browser

## Code Issues Found

### 1. Safe Page - Incorrect CSS Classes

**File:** `crates/adapteros-ui/src/pages/safe.rs:30`
**Issue:** Using `btn btn-outline btn-md` classes which don't match the design system
**Fix:** Should use Button component or proper Tailwind classes

```rust
// Current (WRONG):
<a class="btn btn-outline btn-md" href="/">
    "Try Main App"
</a>

// Should be:
<Button on_click=Callback::new(move |_| {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href("/");
    }
})>
    "Try Main App"
</Button>
```

## Pages to Visually Inspect

### ✅ Dashboard (`/dashboard`)

**Status:** Needs inspection
**Checks:**

- [ ] Page loads without console errors
- [ ] Metrics cards display correctly
- [ ] Charts render (CPU, Memory, GPU)
- [ ] Sparkline metrics show data
- [ ] Status indicators show correct colors
- [ ] Responsive layout works (mobile/tablet/desktop)

**API Dependencies:**

- `/v1/system/overview` - System metrics
- `/v1/system/status` - System status
- SSE stream for real-time updates

### ✅ Adapters (`/adapters`)

**Status:** Needs inspection
**Checks:**

- [ ] Adapter list displays (should show 2 adapters)
- [ ] Table renders correctly
- [ ] "Show more" button works for pagination
- [ ] Click adapter name navigates to detail page
- [ ] Refresh button works
- [ ] Empty state shows if no adapters
- [ ] Error state displays correctly on API failure

**API Dependencies:**

- `/v1/adapters` - List adapters

### ✅ Chat (`/chat`)

**Status:** Needs inspection (CRITICAL - inference testing)
**Checks:**

- [ ] Chat interface loads
- [ ] Message input field works
- [ ] Send button enables/disables correctly
- [ ] Messages display in chat history
- [ ] Streaming tokens appear in real-time
- [ ] Cancel button works during streaming
- [ ] Error messages display on failure
- [ ] Trace panel opens/closes correctly
- [ ] New session button creates new session

**API Dependencies:**

- `/v1/infer/stream` - Streaming inference (requires worker)

### ✅ System (`/system`)

**Status:** Needs inspection
**Checks:**

- [ ] System status displays correctly
- [ ] Health indicators show correct status
- [ ] Memory breakdown charts render
- [ ] Worker status displays
- [ ] Database status shows
- [ ] All tabs work (Overview, Memory, Workers, etc.)

**API Dependencies:**

- `/v1/system/status` - System status
- `/v1/system/overview` - System overview
- `/v1/system/memory` - Memory details

### ✅ Settings (`/settings`)

**Status:** Needs inspection
**Checks:**

- [ ] Settings form loads
- [ ] Form fields are editable
- [ ] Validation works (required fields, formats)
- [ ] Save button works
- [ ] Error messages display on validation failure
- [ ] Success message shows on save

**API Dependencies:**

- `/v1/settings` - Get/update settings

### ✅ Models (`/models`)

**Status:** Needs inspection
**Checks:**

- [ ] Model list displays (should show 2 models)
- [ ] Model details show correctly
- [ ] Status badges display correctly
- [ ] Actions work (load/unload if available)

**API Dependencies:**

- `/v1/models` - List models

### ✅ Training (`/training`)

**Status:** Needs inspection
**Checks:**

- [ ] Training jobs list displays
- [ ] Job status updates correctly
- [ ] Training forms work
- [ ] Create training job form validates
- [ ] Job detail page shows correctly

**API Dependencies:**

- `/v1/jobs` - List training jobs

### ✅ Workers (`/workers`)

**Status:** Needs inspection
**Checks:**

- [ ] Worker list displays
- [ ] Worker status shows correctly
- [ ] Worker detail page works
- [ ] Metrics display correctly

**API Dependencies:**

- `/v1/workers` - List workers

## Common Issues to Check

### JavaScript Console Errors

1. Open DevTools (F12)
2. Check Console tab for:
   - Red error messages
   - Uncaught exceptions
   - Failed API calls
   - WASM loading errors

### Network Tab Issues

1. Open DevTools → Network tab
2. Check for:
   - Failed requests (red status codes)
   - Missing assets (404s)
   - CORS errors
   - Slow API responses

### Visual Rendering Issues

1. **Layout:**

   - Overlapping elements
   - Misaligned components
   - Broken responsive design
   - Missing spacing

2. **Colors:**

   - Status indicators show correct colors
   - Error states are red
   - Success states are green
   - Muted text is readable

3. **Typography:**
   - Text is readable
   - Headings are properly sized
   - Links are distinguishable

### Interactive Elements

1. **Buttons:**

   - Hover states work
   - Click handlers fire
   - Disabled states show correctly
   - Loading states display

2. **Forms:**

   - Input fields are editable
   - Validation messages show
   - Submit buttons work
   - Error states display

3. **Navigation:**
   - Links navigate correctly
   - Browser back/forward works
   - Direct URL access works
   - 404 page shows for invalid routes

### Accessibility

1. **Keyboard Navigation:**

   - Tab through all interactive elements
   - Enter activates buttons/links
   - Escape closes dialogs
   - Ctrl+K opens command palette

2. **Screen Reader:**

   - ARIA labels present
   - Focus indicators visible
   - Semantic HTML used

3. **Color Contrast:**
   - Text meets WCAG AA (4.5:1)
   - UI elements meet WCAG AA (3:1)

## Inference Testing (Requires Worker)

Once worker is built and started:

1. **Navigate to Chat page** (`/chat`)
2. **Create new session** (click "New Session")
3. **Send a test message:**
   ```
   Hello, can you help me test the inference system?
   ```
4. **Verify:**

   - [ ] Message appears in chat
   - [ ] Streaming tokens appear in real-time
   - [ ] Response completes successfully
   - [ ] Trace ID is shown (if available)
   - [ ] Token count displays
   - [ ] Latency metrics show

5. **Test error handling:**
   - [ ] Cancel during streaming works
   - [ ] Network errors display correctly
   - [ ] Invalid requests show error messages

## Browser Compatibility

Test in:

- [ ] Chrome/Edge (Chromium)
- [ ] Safari
- [ ] Firefox

## Performance Checks

- [ ] Initial page load < 2s
- [ ] WASM bundle loads < 1s
- [ ] API responses < 500ms
- [ ] No memory leaks (check DevTools Performance)
- [ ] Smooth scrolling
- [ ] No janky animations

## Next Steps

1. ✅ System started with backend
2. ⏳ Worker building (in progress)
3. ⏳ Visual inspection of all pages
4. ⏳ Fix code issues found
5. ⏳ Test inference once worker ready
6. ⏳ Document all findings
