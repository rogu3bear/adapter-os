import { useCallback } from 'react';
import FeatureLayout from '@/layout/FeatureLayout';
import { DocumentationViewer } from '@/components/DocumentationViewer';
import { DensityProvider } from '@/contexts/DensityContext';
import { useSearchParams } from 'react-router-dom';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { HelpTooltip } from '@/components/ui/help-tooltip';

export default function HelpPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const docId = searchParams.get('doc');
  const { can, userRole } = useRBAC();
  const handleDocChange = useCallback((slug: string) => {
    const next = new URLSearchParams(searchParams);
    next.set('doc', slug);
    setSearchParams(next, { replace: true });
  }, [searchParams, setSearchParams]);

  return (
    <DensityProvider pageKey="help">
      <FeatureLayout 
        title="Documentation" 
        description="Comprehensive documentation and guides for AdapterOS"
      >
        <div className="h-[calc(100vh-12rem)]">
          <DocumentationViewer
            initialDocId={docId || undefined}
            onDocChange={handleDocChange}
          />
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
