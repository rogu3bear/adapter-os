# Model Validation Feature Patch

## Overview
This patch implements model validation and setup guidance for the ModelSelector dropdown component. Users can now see model status at a glance and get explicit commands to resolve setup issues.

## Files Changed (6 files)
- **Backend (3 files)**:
  - `crates/adapteros-server-api/src/types.rs` - Added `ModelValidationResponse` type
  - `crates/adapteros-server-api/src/handlers/models.rs` - Added `validate_model` handler
  - `crates/adapteros-server-api/src/routes.rs` - Added `/v1/models/:model_id/validate` route

- **Frontend (3 files)**:
  - `ui/src/api/types.ts` - Added `ModelValidationResponse` interface
  - `ui/src/api/client.ts` - Added `validateModel()` method
  - `ui/src/components/ModelSelector.tsx` - Enhanced with validation UI

## Features Implemented

### Backend
- ✅ Model validation endpoint (`GET /v1/models/{model_id}/validate`)
- ✅ Checks model existence in database
- ✅ Validates model runtime availability
- ✅ Checks MLX model path existence
- ✅ Validates feature flags
- ✅ Returns actionable download/setup commands

### Frontend
- ✅ Real-time model validation on dropdown load
- ✅ Visual status indicators (✅ Ready, ❌ Needs Setup, 🔄 Validating)
- ✅ Status badges for quick scanning
- ✅ Setup dialog with specific commands
- ✅ One-click command copying with toast feedback
- ✅ Loading states during validation
- ✅ Graceful error handling

## Improvements Made
1. ✅ Toast notifications for copy feedback
2. ✅ Removed unused imports
3. ✅ Added loading states
4. ✅ Improved error handling
5. ✅ Fixed compiler warnings

## Applying the Patch

```bash
git apply model-validation-patch-v2.patch
```

Or manually review and apply:
```bash
git apply --check model-validation-patch-v2.patch  # Check if patch applies cleanly
git apply model-validation-patch-v2.patch          # Apply the patch
```

## Known Limitations
- No caching (validates every time)
- Basic validation (existence, not integrity)
- No retry logic
- No tests included

## Testing
After applying, test by:
1. Opening the model dropdown
2. Checking status indicators
3. Clicking setup button for unavailable models
4. Copying setup commands
