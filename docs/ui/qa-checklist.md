# UI QA Checklist

This checklist validates UI functionality after changes to navigation, cross-linking, or data flow.

## Navigation

### StartMenu Items

Verify each module opens correctly:

| Module | Route | Expected |
|--------|-------|----------|
| Dashboard | `/` | Dashboard with Quick Actions, status cards, activity feed |
| Chat | `/chat` | Chat interface with session management |
| Runs | `/runs` | Run list with split panel detail view |
| Audit | `/audit` | Multi-tab audit viewer (Timeline, HashChain, MerkleTree, Compliance, Embeddings) |
| Training | `/training` | Training jobs list with filters |
| Datasets | `/datasets` | Dataset management |
| Adapters | `/adapters` | Adapter registry |
| Stacks | `/stacks` | Stack configuration |
| Policies | `/policies` | Policy packs |
| Routing | `/routing` | Routing rules and decisions |
| Workers | `/workers` | Worker management |
| System | `/system` | System status and health |
| Monitoring | `/monitoring` | Alerts and metrics |
| Settings | `/settings` | User and system settings |

### Deep Link Checks

Verify these URLs work:

| URL | Expected Behavior |
|-----|-------------------|
| `/runs/{trace_id}` | Opens Run Detail at Overview tab |
| `/runs/{trace_id}?tab=trace` | Opens Run Detail at Trace tab |
| `/runs/{trace_id}?tab=receipt` | Opens Run Detail at Receipt tab |
| `/runs/{trace_id}?tab=routing` | Opens Run Detail at Routing tab |
| `/runs/{trace_id}?tab=diff` | Opens Run Detail at Diff tab |
| `/runs/{trace_id}?tab=diff&compare={other_id}` | Opens Diff tab with comparison loaded |
| `/diff?run={trace_id}` | Redirects to `/runs/{trace_id}?tab=diff` |
| `/diff?run_a={id}&run_b={id2}` | Redirects to `/runs/{id}?tab=diff&compare={id2}` |
| `/chat/{session_id}` | Opens specific chat session |
| `/adapters/{adapter_id}` | Opens adapter detail |
| `/training/{job_id}` | Opens training job detail |

## Happy Paths

### 1. Run Inference → Inspect Output → Open Run Detail → Verify Receipt

Steps:
1. [ ] Navigate to `/chat`
2. [ ] Send a message to the chat
3. [ ] Wait for assistant response to complete
4. [ ] Verify "Run" and "Receipt" links appear below the response
5. [ ] Click "Run" link
6. [ ] Verify `/runs/{trace_id}` opens with Overview tab
7. [ ] Verify Configuration section shows (with "Unknown" if not captured)
8. [ ] Click "Receipt" tab
9. [ ] Verify receipt hashes are displayed
10. [ ] Verify "Verified" badge shows if verification passed

### 2. Configure Stack/Policy → Run → See Active Configuration

Steps:
1. [ ] Navigate to `/stacks`
2. [ ] Note the active stack configuration
3. [ ] Navigate to `/chat` and run inference
4. [ ] Open Run Detail for the run
5. [ ] Verify Overview tab shows Configuration section
6. [ ] Note: Currently shows "Unknown" (backend doesn't capture yet)

### 3. Operate Health → Drill Down → Return to Dashboard

Steps:
1. [ ] Navigate to `/` (Dashboard)
2. [ ] Note system status in status cards
3. [ ] If alerts are visible, click "View Alerts" quick action
4. [ ] Verify `/monitoring` opens
5. [ ] Click an alert to see detail
6. [ ] Use back navigation or menu to return to Dashboard
7. [ ] Verify Dashboard state is preserved

### 4. Workers Spawn: Quick (Default) and Advanced

Steps:
1. [ ] Navigate to `/workers`
2. [ ] With at least one node and deployment config present, click "Spawn Worker"
3. [ ] Verify quick spawn runs without opening advanced fields and shows success notification
4. [ ] Verify workers list refreshes and new worker appears
5. [ ] Click "Advanced Spawn" to open the dialog
6. [ ] Verify mode defaults to "Quick" and shows a concise "Quick will choose" summary
7. [ ] Switch mode to "Advanced" and verify Node, Deployment Config, and Socket Path fields are visible
8. [ ] Verify submit is blocked when required Advanced fields are empty/invalid
9. [ ] Set a custom Socket Path, then change Node; verify custom path is preserved
10. [ ] Submit and verify dialog closes, success notification appears, and workers list refreshes

## Error States

### Backend Unavailable

1. [ ] Stop the backend server
2. [ ] Refresh any page
3. [ ] Verify error message appears (not blank screen)
4. [ ] Verify `ErrorDisplay` component shows with readable message

### Auth Expired

1. [ ] Clear auth token from storage
2. [ ] Navigate to protected route
3. [ ] Verify redirect to login or auth error message

### Invalid Route

1. [ ] Navigate to `/nonexistent`
2. [ ] Verify 404 or redirect to Dashboard

### Invalid Run ID

1. [ ] Navigate to `/runs/invalid-id-12345`
2. [ ] Verify error state shows "Run not found" or similar

## Cross-Linking

### Chat → Runs

- [ ] Assistant message with trace_id shows "Run" link
- [ ] Assistant message with trace_id shows "Receipt" link
- [ ] Links navigate to correct `/runs/{trace_id}` URL
- [ ] TraceButton opens modal with "Open Full View" link

### Audit → Runs

- [ ] Audit timeline with resource_type "inference" shows clickable resource_id
- [ ] Clicking resource_id navigates to `/runs/{id}`

### Diff → Runs

- [ ] `/diff` with query params redirects to Run Detail diff tab
- [ ] "Open in Run Detail" link works from diff page

### Run Detail Cross-Tabs

- [ ] Overview has Provenance links to other tabs
- [ ] Each tab link updates URL query param
- [ ] Browser back/forward works with tab navigation

## Quick Actions

### Dashboard Quick Actions

- [ ] "New Run" links to `/chat`
- [ ] "Verify Receipt" links to `/runs`
- [ ] "Activate Stack" links to `/stacks`
- [ ] "Upload Document" links to `/datasets`
- [ ] "View Alerts" links to `/monitoring` (if permission)

### Run Detail Quick Actions

- [ ] "Copy Run ID" copies to clipboard, shows "Copied!" feedback
- [ ] "Copy Receipt Hash" copies hash format to clipboard
- [ ] "Export" opens export endpoint
- [ ] "Open Diff" navigates to diff tab

## Mobile Layout

Test at viewport widths: 320px, 375px, 414px

- [ ] StartMenu opens as overlay
- [ ] Navigation items are tappable
- [ ] Chat input doesn't overlap controls
- [ ] Tables scroll horizontally
- [ ] Cards stack vertically
- [ ] Quick Actions wrap properly

## Performance

- [ ] Dashboard loads within 3 seconds
- [ ] Run list loads within 2 seconds
- [ ] Tab switches are instant (no full reload)
- [ ] Polling doesn't cause visible jank

## Accessibility

- [ ] Tab navigation works through page
- [ ] Focus indicators are visible
- [ ] ARIA labels on interactive elements
- [ ] Color contrast meets WCAG AA
- [ ] Error messages are screen-reader friendly

## Data Consistency

- [ ] Polling updates reflect server state
- [ ] Refetch button updates data
- [ ] No duplicate data in lists after refetch
- [ ] Loading states show during fetch

## Terminology Consistency

Verify these terms are used consistently:

- [ ] "Runs" (not "Flight Recorder" or "Diagnostics")
- [ ] "Receipt" (not "Proof" or "Verification")
- [ ] "Trace" (not "Span" in UI)
- [ ] Status badges match: running, completed, failed, cancelled
