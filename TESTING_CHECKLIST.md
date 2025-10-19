# Manual Testing Checklist - Base Model UI Journey

## Test Environment Setup
- [ ] Backend server running (`cargo run --release --bin adapteros-server`)
- [ ] Database migrated (`migrations/0042_base_model_ui_support.sql` applied)
- [ ] UI dev server running (`cd ui && pnpm dev`)
- [ ] Test tenant created and configured
- [ ] Test user logged in with appropriate role

---

## Backend API Testing

### 1. Model Import Endpoint
**Endpoint:** `POST /v1/models/import`

**Test Cases:**
- [ ] **TC1.1:** Import with valid paths succeeds
  ```bash
  curl -X POST http://localhost:8080/api/v1/models/import \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d '{
      "model_name": "test-model",
      "weights_path": "/path/to/weights.safetensors",
      "config_path": "/path/to/config.json",
      "tokenizer_path": "/path/to/tokenizer.json"
    }'
  ```
  **Expected:** 200 OK, returns import_id and status "validating"

- [ ] **TC1.2:** Import with non-existent paths fails
  **Expected:** 400 Bad Request, error "weights file not found"

- [ ] **TC1.3:** Import without admin role fails
  **Expected:** 401 Unauthorized

- [ ] **TC1.4:** Import record created in database
  ```sql
  SELECT * FROM base_model_imports WHERE model_name = 'test-model';
  ```
  **Expected:** Record exists with status "validating"

### 2. Model Load Endpoint
**Endpoint:** `POST /v1/models/{model_id}/load`

**Test Cases:**
- [ ] **TC2.1:** Load existing model succeeds
  ```bash
  curl -X POST http://localhost:8080/api/v1/models/test-model-id/load \
    -H "Authorization: Bearer $TOKEN"
  ```
  **Expected:** 200 OK, status changes to "loaded"

- [ ] **TC2.2:** Load non-existent model fails
  **Expected:** 404 Not Found

- [ ] **TC2.3:** Load without operator/admin role fails
  **Expected:** 401 Unauthorized

- [ ] **TC2.4:** base_model_status table updated
  ```sql
  SELECT status, loaded_at, memory_usage_mb FROM base_model_status WHERE model_id = 'test-model-id';
  ```
  **Expected:** status = "loaded", loaded_at populated, memory_usage_mb > 0

- [ ] **TC2.5:** Journey step "model_loaded" tracked
  ```sql
  SELECT * FROM onboarding_journeys WHERE step_completed = 'model_loaded';
  ```
  **Expected:** Record exists

### 3. Model Unload Endpoint
**Endpoint:** `POST /v1/models/{model_id}/unload`

**Test Cases:**
- [ ] **TC3.1:** Unload loaded model succeeds
  **Expected:** 200 OK, status changes to "unloaded"

- [ ] **TC3.2:** Unload without operator/admin role fails
  **Expected:** 401 Unauthorized

- [ ] **TC3.3:** base_model_status table updated
  **Expected:** status = "unloaded", loaded_at = NULL, memory_usage_mb = NULL

### 4. Import Status Endpoint
**Endpoint:** `GET /v1/models/imports/{import_id}`

**Test Cases:**
- [ ] **TC4.1:** Get status of existing import
  **Expected:** 200 OK, returns import status and progress

- [ ] **TC4.2:** Get status of non-existent import
  **Expected:** 404 Not Found

### 5. Cursor Config Endpoint
**Endpoint:** `GET /v1/models/cursor-config`

**Test Cases:**
- [ ] **TC5.1:** Get config with loaded model
  **Expected:** 200 OK, is_ready = true, valid endpoint and model name

- [ ] **TC5.2:** Get config without loaded model
  **Expected:** 200 OK, is_ready = false

- [ ] **TC5.3:** Setup instructions include 6 steps
  **Expected:** Array with 6 instruction strings

---

## Frontend UI Testing

### 6. Model Import Wizard
**Component:** `ModelImportWizard.tsx`

**Test Cases:**
- [ ] **TC6.1:** Wizard opens from dashboard
  **Action:** Click "Import New Model" button
  **Expected:** Modal opens with 4-step wizard

- [ ] **TC6.2:** Step 1 - Model name validation
  **Action:** Try to proceed without entering name
  **Expected:** Toast error "Model name is required"

- [ ] **TC6.3:** Step 2 - Weights path validation
  **Action:** Enter path without .safetensors extension
  **Expected:** Toast error "Weights file must be .safetensors format"

- [ ] **TC6.4:** Step 3 - Config paths validation
  **Action:** Leave config path empty and proceed
  **Expected:** Toast error "Config and tokenizer paths are required"

- [ ] **TC6.5:** Step 4 - Review displays all inputs
  **Expected:** All entered values visible in review section

- [ ] **TC6.6:** Import submission succeeds
  **Action:** Click "Import Model"
  **Expected:** Toast success with import_id, wizard closes

- [ ] **TC6.7:** Cancel button works
  **Action:** Click "Cancel" at any step
  **Expected:** Wizard closes without importing

### 7. Base Model Loader
**Component:** `BaseModelLoader.tsx`

**Test Cases:**
- [ ] **TC7.1:** Load button enabled when model unloaded
  **Expected:** "Load Model" button clickable, not disabled

- [ ] **TC7.2:** Load button triggers API call
  **Action:** Click "Load Model"
  **Expected:** Loading indicator, then toast success

- [ ] **TC7.3:** Unload button enabled when model loaded
  **Expected:** "Unload Model" button clickable after load

- [ ] **TC7.4:** Unload button triggers API call
  **Action:** Click "Unload Model"
  **Expected:** Loading indicator, then toast success

- [ ] **TC7.5:** Status icon updates
  **Expected:** 
    - Unloaded: X icon (gray)
    - Loading: Spinning refresh icon (blue)
    - Loaded: Check icon (green)

- [ ] **TC7.6:** Badge displays correct status
  **Expected:** Badge shows "Loaded" (green) or "Unloaded" (gray)

- [ ] **TC7.7:** Import wizard triggers from loader
  **Action:** Click "Import New Model"
  **Expected:** ModelImportWizard modal opens

### 8. Cursor Setup Wizard
**Component:** `CursorSetupWizard.tsx`

**Test Cases:**
- [ ] **TC8.1:** Wizard loads Cursor config on mount
  **Expected:** API call to `/v1/models/cursor-config` on open

- [ ] **TC8.2:** Step 1 - Prerequisites validation
  **Action:** Try to proceed without loaded model
  **Expected:** Toast error "Please load a base model first"

- [ ] **TC8.3:** Step 2 - API endpoint copy works
  **Action:** Click copy button for endpoint
  **Expected:** Toast success "Copied to clipboard"

- [ ] **TC8.4:** Step 3 - Model name copy works
  **Action:** Click copy button for model name
  **Expected:** Toast success "Copied to clipboard"

- [ ] **TC8.5:** Step 4 - Instructions display
  **Expected:** 6 numbered setup steps visible

- [ ] **TC8.6:** Open Cursor Settings button works
  **Action:** Click "Open Cursor Settings"
  **Expected:** New tab opens to cursor.sh/settings

- [ ] **TC8.7:** Complete button closes wizard
  **Action:** Click "Complete Setup"
  **Expected:** Toast success, wizard closes

### 9. Dashboard Integration
**Component:** `Dashboard.tsx`

**Test Cases:**
- [ ] **TC9.1:** BaseModelStatusComponent displays
  **Expected:** Component visible in dashboard

- [ ] **TC9.2:** BaseModelLoader displays
  **Expected:** Component visible next to status

- [ ] **TC9.3:** Cursor setup button visible
  **Expected:** "Configure Cursor IDE" button in dashboard

- [ ] **TC9.4:** Cursor wizard opens from dashboard
  **Action:** Click "Configure Cursor IDE"
  **Expected:** CursorSetupWizard modal opens

- [ ] **TC9.5:** Model status refreshes on load/unload
  **Action:** Load model via BaseModelLoader
  **Expected:** BaseModelStatusComponent updates automatically

---

## End-to-End Journey Testing

### 10. Complete User Journey
**Scenario:** New user imports model, loads it, and configures Cursor

**Test Cases:**
- [ ] **TC10.1:** Import → Load → Configure flow
  **Steps:**
  1. Open dashboard
  2. Click "Import New Model"
  3. Complete wizard with valid paths
  4. Wait for import to complete
  5. Click "Load Model"
  6. Verify status shows "Loaded"
  7. Click "Configure Cursor IDE"
  8. Complete Cursor setup wizard
  9. Verify all steps tracked in onboarding_journeys
  
  **Expected:** All steps succeed, journey steps recorded

- [ ] **TC10.2:** Journey tracking verification
  ```sql
  SELECT step_completed, completed_at 
  FROM onboarding_journeys 
  WHERE user_id = 'test-user' 
  ORDER BY completed_at;
  ```
  **Expected:** Rows for: model_imported, model_loaded, cursor_configured

- [ ] **TC10.3:** Real Cursor IDE connection test
  **Steps:**
  1. Open Cursor IDE
  2. Go to Settings → Models
  3. Add custom endpoint from wizard
  4. Set model name from wizard
  5. Test connection
  6. Try code completion
  
  **Expected:** Connection successful, completions work

---

## Error Handling & Edge Cases

### 11. Error Scenarios
- [ ] **TC11.1:** Network error during import
  **Expected:** Toast error with message

- [ ] **TC11.2:** Unauthorized access attempt
  **Expected:** Redirect to login

- [ ] **TC11.3:** Model already loaded
  **Expected:** Load button disabled

- [ ] **TC11.4:** Model already unloaded
  **Expected:** Unload button disabled

- [ ] **TC11.5:** Import with invalid JSON
  **Expected:** 400 Bad Request

- [ ] **TC11.6:** Concurrent load operations
  **Expected:** Second request queued or rejected

---

## Performance Testing

### 12. Performance Benchmarks
- [ ] **TC12.1:** Import API response time < 500ms
- [ ] **TC12.2:** Load API response time < 200ms
- [ ] **TC12.3:** Wizard step transitions < 100ms
- [ ] **TC12.4:** Status polling updates < 1s
- [ ] **TC12.5:** Dashboard initial load < 3s

---

## Browser Compatibility

### 13. Cross-Browser Testing
- [ ] **TC13.1:** Chrome/Edge (latest)
- [ ] **TC13.2:** Firefox (latest)
- [ ] **TC13.3:** Safari (macOS)

---

## Accessibility Testing

### 14. A11y Compliance
- [ ] **TC14.1:** Keyboard navigation works
- [ ] **TC14.2:** Screen reader announces steps
- [ ] **TC14.3:** Focus indicators visible
- [ ] **TC14.4:** Color contrast meets WCAG AA

---

## Test Results Summary

| Category | Passed | Failed | Skipped | Notes |
|----------|--------|--------|---------|-------|
| Backend API | /17 | /17 | /17 | |
| Frontend UI | /25 | /25 | /25 | |
| E2E Journey | /3 | /3 | /3 | |
| Error Handling | /6 | /6 | /6 | |
| Performance | /5 | /5 | /5 | |
| Browser | /3 | /3 | /3 | |
| Accessibility | /4 | /4 | /4 | |
| **Total** | **/63** | **/63** | **/63** | |

---

## Sign-off

- [ ] All critical tests passed
- [ ] Known issues documented
- [ ] Ready for production deployment

**Tester:** ________________  
**Date:** ________________  
**Signature:** ________________

