import { useAuth } from '@/providers/CoreProviders';
import { useTenant } from '@/providers/FeatureProviders';
import FeatureLayout from '@/layout/FeatureLayout';
import { InferencePlayground } from '@/components/InferencePlayground';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { PERMISSIONS } from '@/utils/rbac';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { ShieldAlert } from 'lucide-react';
import { Link } from 'react-router-dom';

export default function InferencePage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const { can } = useRBAC();

  const canExecuteInference = can(PERMISSIONS.INFERENCE_EXECUTE);

  return (
    <DensityProvider pageKey="inference">
      <FeatureLayout title="Inference" description="Run inference with loaded adapters">
        {!canExecuteInference ? (
          <Alert variant="destructive">
            <ShieldAlert className="h-4 w-4" />
            <AlertDescription>
              You do not have permission to execute inference. Required permission: inference:execute
            </AlertDescription>
          </Alert>
        ) : (
          <InferencePlayground selectedTenant={selectedTenant} />
        )}
        <div className="mt-4 text-sm text-muted-foreground">
          <Link to="/telemetry/viewer" className="underline underline-offset-4">
            View telemetry for this session in Telemetry Viewer
          </Link>
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
