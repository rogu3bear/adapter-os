# Accessibility Shortcuts

Use these conventions whenever a feature registers a keyboard shortcut. The shared `useKeyboardShortcuts` helper in `ui/src/utils/accessibility.ts` listens for global `/` (search) and `?` (help) bindings while respecting focused inputs.

- `/` opens the command palette or search affordance. `useKeyboardShortcuts` prevents typing interference by ignoring events from `input`, `textarea`, `select`, and content-editable regions. When adding new search boxes, wire their trigger to the shared palette instead of another listener.
- `?` (Shift + `/`) opens contextual help. The handler should toggle the shared `HelpCenter` to keep announcements consistent.
- Avoid attaching page-level `onKeyDown` handlers that consume `/` or `?` on interactive elements (forms, dialogs). If a component must handle these keys, wrap it in a callback that calls `event.stopPropagation()` only after checking for modifier keys.
- When a page introduces additional shortcuts, document them alongside these defaults and add automated focus guards similar to `useKeyboardShortcuts` to protect screen reader input.
- Controls rendered inside dialogs should not block the palette shortcut; leave global listeners active so users can still search for destinations while a modal is open.

Annotate components that manage raw keyboard events with comments referencing these conventions so reviewers can confirm they do not override the shared bindings.
