import { CoreProviders } from './CoreProviders';
import { FeatureProviders } from './FeatureProviders';
import { BookmarkProvider } from '@/contexts/BookmarkContext';
import { UndoRedoProvider } from '@/contexts/UndoRedoContext';
import { UndoRedoToolbar } from '@/components/ui/undo-redo-toolbar';

/**
 * Combined provider wrapper for all application-level contexts.
 * Structure: CoreProviders (Theme, Auth, Resize) → FeatureProviders (Tenant) → BookmarkProvider → UndoRedoProvider
 * 
 * Note: BreadcrumbProvider removed (breadcrumbs now derived statelessly from URL)
 * Note: CommandPaletteProvider is initialized in RootLayout where navigation routes are available
 */
export function AppProviders({ children }: { children: React.ReactNode }) {
  return (
    <CoreProviders>
      <FeatureProviders>
        <BookmarkProvider>
          <UndoRedoProvider>
            {children}
            <UndoRedoToolbar />
          </UndoRedoProvider>
        </BookmarkProvider>
      </FeatureProviders>
    </CoreProviders>
  );
}

