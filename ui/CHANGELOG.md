# Changelog

All notable changes to the AdapterOS UI are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Document-Chat Integration**: New split-view layout for chatting with documents
  - Document-specific chat page at `/documents/:id/chat`
  - PDF viewer with evidence navigation
  - Collection selector in chat interface
  - Context-aware chat for document Q&A

- **Collection-Session Binding**: Chat sessions can now be scoped to document collections
  - RAG retrieval scoped to selected collection
  - Session persists collection binding
  - Collection selector in ChatInterface
  - Evidence panel shows relevant document excerpts

- **Role-Specific Dashboards**: Tailored dashboards for each user role
  - **AdminDashboard**: Tenant, user, security management
  - **OperatorDashboard**: Training, datasets, adapters
  - **SREDashboard**: Nodes, workers, performance
  - **ComplianceDashboard**: Audit, policies, violations
  - **ViewerDashboard**: Read-only overview
  - Dynamic dashboard routing based on user role

- **Policy Enforcement UI**: Pre-flight policy checks before operations
  - PolicyPreflightDialog component
  - Integration with adapter loading and stack activation
  - Admin override capability
  - Visual policy violation feedback

- **Progressive Disclosure UX**: Simplified router details
  - RouterSummaryView for non-technical users
  - RouterTechnicalView for detailed analysis
  - Tabbed interface in RouterDetailsModal
  - Terminology constants for role-based language

- **Settings Persistence**: Backend-integrated settings management
  - useSettings and useUpdateSettings hooks
  - Optimistic updates with restart-required handling
  - Per-tenant settings storage
  - Server restart notifications

- **Training Flow Improvements**
  - "Add to Stack" from adapter and training job pages
  - Clickable collection names in TrainingSnapshotPanel
  - Direct navigation from training jobs to adapter details
  - Stack creation workflow from trained adapters

- **Document Library**: Complete document management interface
  - Document upload and organization
  - Collection creation and management
  - PDF preview and viewer
  - Evidence extraction and navigation

- **Evidence System**: Structured evidence tracking
  - EvidencePanel component for chat responses
  - EvidenceItem with document references
  - ProofBadge for evidence quality indicators
  - Navigation to source documents

### Enhanced

- **DatasetBuilder Tooltip:** Added guidance clarifying DatasetBuilder as advanced path vs. Training Wizard for simple uploads
- **Chat Context Panel:** Enhanced "Currently Loaded" panel with default stack badge, lifecycle/description details, and base model placeholder
  - Default stack badge when applicable
  - Stack description or lifecycle state
  - Adapter count and composition
  - Collapsible design for minimal UI footprint

- **ChatInterface Refactoring**
  - React Query integration for session management
  - Collection context support
  - Evidence panel integration
  - Improved error handling and loading states

### Changed

- Refactored DocumentLibrary to use React Query hooks
- Refactored CollectionManager to use React Query hooks
- Updated ChatInterface with collection context support
- Enhanced Dashboard component with role-based routing
- Improved API client with new endpoints for documents, collections, evidence

### Accessibility

- Fixed nested interactive elements in DatasetsTab (HelpTooltip separated from Button element)
- Added ARIA labels and semantic structure to stack context panel
- Implemented proper `aria-expanded` attribute on collapsible sections
- Verified color contrast meets WCAG AA standards for badges and informational text
- Enhanced keyboard navigation in document viewer
- Screen reader support for evidence navigation

### Bug Fixes

- Removed duplicate `debouncedUpdateSession` declaration in ChatInterface
- Improved defensive null checks for stack comparison
- Added graceful fallback to "Unknown" when stack details are unavailable
- Fixed tooltip positioning to avoid visual obstruction
- Fixed collection selector state management in chat sessions
- Corrected evidence panel rendering with empty states

### Technical

- **New API Types**:
  - `document-types.ts` with Document, Collection, Evidence interfaces
  - `chat-types.ts` with ChatSession, ChatMessage types
  - Extended inference types for collection binding

- **New Hooks**:
  - `useDocumentsApi` - Document CRUD operations
  - `useCollectionsApi` - Collection management
  - `useEvidenceApi` - Evidence retrieval
  - `useSettings` - Settings management
  - `useUpdateSettings` - Settings persistence
  - `useChatSessionsApi` - Chat session operations

- **New Contexts**:
  - `DocumentViewerContext` - Document viewing state
  - Extended chat context for collection binding

- **New Components**:
  - `DocumentLibrary/` - Document management pages
  - `collections/` - Collection UI components
  - `documents/` - Document viewer components
  - `chat/EvidencePanel` - Evidence display
  - `chat/EvidenceItem` - Individual evidence entries
  - `chat/ProofBadge` - Evidence quality indicators
  - `PolicyPreflightDialog` - Policy enforcement UI
  - `RouterSummaryView` - Simplified router view
  - `RouterTechnicalView` - Detailed router analysis

- **New Constants**:
  - `src/constants/terminology.ts` - Progressive disclosure labels
  - `src/constants/index.ts` - Shared constants

- **Configuration Updates**:
  - New routes in `src/config/routes.ts` for documents and chat
  - Extended API client with document/collection/evidence endpoints

### UI Components Modified

- `ui/src/pages/Training/DatasetsTab.tsx` - Added HelpTooltip on "Upload Dataset" button
- `ui/src/components/ChatInterface.tsx` - Enhanced chat context panel, collection binding, evidence panel
- `ui/src/components/Dashboard.tsx` - Role-based dashboard routing
- `ui/src/pages/Adapters/AdapterDetailPage.tsx` - "Add to Stack" functionality
- `ui/src/pages/Training/TrainingPage.tsx` - Stack creation from training jobs
- `ui/src/api/client.ts` - Extended API endpoints

### Migration Notes

- Chat sessions now support optional `collection_id` field for scoped RAG retrieval
- Settings persistence requires backend support at `/v1/settings` endpoint
- Document library requires backend endpoints at `/v1/documents`, `/v1/collections`, `/v1/evidence`
- Policy enforcement requires `/v1/policies/validate` endpoint

### Performance Improvements

- React Query caching for documents and collections
- Optimistic updates for settings changes
- Lazy loading of document viewer components
- Debounced search in document library

## References

- See [../AGENTS.md](../AGENTS.md) for UI Integration Patterns documentation
- See [../docs/UI_INTEGRATION.md](../docs/UI_INTEGRATION.md) for detailed UI architecture
- Component locations:
  - Dataset upload: `src/pages/Training/DatasetsTab.tsx`
  - Chat context: `src/components/ChatInterface.tsx`
