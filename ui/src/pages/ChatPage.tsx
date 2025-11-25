import { useAuth, useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { ChatInterface } from '@/components/ChatInterface';
import { ChatErrorBoundary } from '@/components/chat/ChatErrorBoundary';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { PERMISSIONS } from '@/utils/rbac';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { ShieldAlert } from 'lucide-react';
import { useSearchParams } from 'react-router-dom';

export default function ChatPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const { can } = useRBAC();
  const [searchParams] = useSearchParams();

  const canExecuteInference = can(PERMISSIONS.INFERENCE_EXECUTE);
  const initialStackId = searchParams.get('stack') || undefined;

  return (
    <DensityProvider pageKey="chat">
      <FeatureLayout title="Chat" description="Conversational interface with adapter stacks">
        {!canExecuteInference ? (
          <Alert variant="destructive">
            <ShieldAlert className="h-4 w-4" />
            <AlertDescription>
              You do not have permission to execute inference. Required permission: inference:execute
            </AlertDescription>
          </Alert>
        ) : (
          <div className="h-[calc(100vh-200px)] border rounded-lg overflow-hidden">
            <ChatErrorBoundary>
              <ChatInterface selectedTenant={selectedTenant} initialStackId={initialStackId} />
            </ChatErrorBoundary>
          </div>
        )}
      </FeatureLayout>
    </DensityProvider>
  );
}

