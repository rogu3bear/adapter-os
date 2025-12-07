import { Suspense, useState } from 'react';
import { useAuth } from '@/providers/CoreProviders';
import { useTenant } from '@/providers/FeatureProviders';
import PageWrapper from '@/layout/PageWrapper';
import { ChatInterface } from '@/components/ChatInterface';
import { ChatErrorBoundary } from '@/components/chat/ChatErrorBoundary';
import { useRBAC } from '@/hooks/useRBAC';
import { PERMISSIONS } from '@/utils/rbac';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { ShieldAlert } from 'lucide-react';
import { useSearchParams, Link } from 'react-router-dom';
import { CollapsibleSidebar } from '@/pages/OwnerHome/components/CollapsibleSidebar';
import { SimplifiedChatWidget } from '@/components/chat/SimplifiedChatWidget';
import { ChatSkeleton } from '@/components/skeletons/ChatSkeleton';
import { Switch } from '@/components/ui/switch';

export default function ChatPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const { can } = useRBAC();
  const [searchParams] = useSearchParams();
  const [streamMode, setStreamMode] = useState<'tokens' | 'chunks'>('tokens');
  const [developerMode, setDeveloperMode] = useState(false);

  const canExecuteInference = can(PERMISSIONS.INFERENCE_EXECUTE);
  const initialStackId = searchParams.get('stack') || undefined;

  return (
    <PageWrapper
      pageKey="chat"
      title="Chat"
      description="Conversational interface with adapter stacks"
    >
      {!canExecuteInference ? (
        <Alert variant="destructive">
          <ShieldAlert className="h-4 w-4" />
          <AlertDescription>
            You do not have permission to execute inference. Required permission: inference:execute
          </AlertDescription>
        </Alert>
      ) : (
        <>
          <div className="flex items-center gap-4 mb-4">
            <div className="flex items-center gap-2">
              <Switch
                id="stream-mode"
                checked={streamMode === 'tokens'}
                onCheckedChange={(checked) => setStreamMode(checked ? 'tokens' : 'chunks')}
              />
              <label htmlFor="stream-mode" className="text-sm text-muted-foreground">
                Stream mode: {streamMode === 'tokens' ? 'tokens' : 'chunks'}
              </label>
            </div>
            <div className="flex items-center gap-2">
              <Switch
                id="developer-mode"
                checked={developerMode}
                onCheckedChange={setDeveloperMode}
              />
              <label htmlFor="developer-mode" className="text-sm text-muted-foreground">
                Developer mode
              </label>
            </div>
          </div>
          <div className="flex h-[calc(100vh-calc(var(--base-unit)*50))] gap-4">
          {/* Main Chat Interface */}
          <div className="flex-1 border rounded-lg overflow-hidden min-w-0">
              <ChatErrorBoundary>
                <Suspense fallback={<ChatSkeleton />}>
                  <ChatInterface
                    selectedTenant={selectedTenant}
                    initialStackId={initialStackId}
                    streamMode={streamMode}
                    developerMode={developerMode}
                  />
                </Suspense>
              </ChatErrorBoundary>
          </div>

          {/* Slide-out Chat Widget */}
          <CollapsibleSidebar defaultExpanded={false} className="h-full">
            <SimplifiedChatWidget selectedTenant={selectedTenant} />
          </CollapsibleSidebar>
          </div>
        </>
      )}
      <div className="mt-4 text-sm text-muted-foreground">
        <Link to="/telemetry/viewer" className="underline underline-offset-4">
          View telemetry for this chat session
        </Link>
      </div>
    </PageWrapper>
  );
}

