import { useAuth } from '@/providers/CoreProviders';
import { useTenant } from '@/providers/FeatureProviders';
import PageWrapper from '@/layout/PageWrapper';
import { InferencePlayground } from '@/components/InferencePlayground';
import { useRBAC } from '@/hooks/security/useRBAC';
import { PERMISSIONS } from '@/utils/rbac';
import { PermissionDenied } from '@/components/ui/permission-denied';
import { Link } from 'react-router-dom';
import { buildTelemetryViewerLink } from '@/utils/navLinks';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { withPageErrorBoundary } from '@/components/ui/with-page-error-boundary';

function InferencePage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const { can } = useRBAC();

  const canExecuteInference = can(PERMISSIONS.INFERENCE_EXECUTE);

  return (
    <PageWrapper pageKey="inference" title="Inference" description="Run inference with loaded adapters">
      {!canExecuteInference ? (
        <PermissionDenied
          requiredPermission={PERMISSIONS.INFERENCE_EXECUTE}
          requiredRoles={['admin', 'operator', 'developer']}
        />
      ) : (
        <SectionErrorBoundary sectionName="Inference Playground">
          <InferencePlayground selectedTenant={selectedTenant} />
        </SectionErrorBoundary>
      )}
      <div className="mt-4 text-sm text-muted-foreground">
        <Link to={buildTelemetryViewerLink()} className="underline underline-offset-4">
          View telemetry for this session in Telemetry Viewer
        </Link>
      </div>
    </PageWrapper>
  );
}

export default withPageErrorBoundary(InferencePage, { pageName: 'Inference' });
