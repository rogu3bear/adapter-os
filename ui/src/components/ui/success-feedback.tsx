//! Success Feedback Component
//!
//! Provides rich success feedback with next steps guidance to build user confidence.
//!
//! Citations:
//! - docs/Smashing Design Techniques.md L300-L350 - Trust-building UX patterns
//! - ui/src/components/SingleFileAdapterTrainer.tsx L550-L600 - Current success handling

import React from 'react';
import { Alert, AlertDescription, AlertTitle } from './alert';
import { Button } from './button';
import { Badge } from './badge';
import { CheckCircle, ArrowRight, Sparkles } from 'lucide-react';

export interface NextStep {
  label: string;
  action?: () => void;
  route?: string;
  primary?: boolean;
}

export interface SuccessFeedbackProps {
  title: string;
  description: string;
  nextSteps?: NextStep[];
  variant?: 'default' | 'celebration' | 'milestone';
  className?: string;
  autoHide?: boolean;
  hideDelay?: number;
}

export function SuccessFeedback({
  title,
  description,
  nextSteps = [],
  variant = 'default',
  className = '',
  autoHide = false,
  hideDelay = 5000
}: SuccessFeedbackProps) {
  const [visible, setVisible] = React.useState(true);

  React.useEffect(() => {
    if (autoHide && nextSteps.length === 0) {
      const timer = setTimeout(() => setVisible(false), hideDelay);
      return () => clearTimeout(timer);
    }
  }, [autoHide, hideDelay, nextSteps.length]);

  if (!visible) return null;

  const getIcon = () => {
    switch (variant) {
      case 'celebration':
        return <Sparkles className="h-5 w-5 text-gray-500" />;
      case 'milestone':
        return <CheckCircle className="h-5 w-5 text-gray-400" />;
      default:
        return <CheckCircle className="h-5 w-5 text-gray-600" />;
    }
  };

  const getAlertClass = () => {
    switch (variant) {
      case 'celebration':
        return 'border-yellow-200 bg-yellow-50';
      case 'milestone':
        return 'border-blue-200 bg-blue-50';
      default:
        return 'border-green-200 bg-green-50';
    }
  };

  const getTitleClass = () => {
    switch (variant) {
      case 'celebration':
        return 'text-yellow-800';
      case 'milestone':
        return 'text-blue-800';
      default:
        return 'text-green-800';
    }
  };

  const getDescriptionClass = () => {
    switch (variant) {
      case 'celebration':
        return 'text-yellow-700';
      case 'milestone':
        return 'text-blue-700';
      default:
        return 'text-green-700';
    }
  };

  return (
    <Alert className={`${getAlertClass()} ${className}`}>
      {getIcon()}
      <div className="flex-1">
        <AlertTitle className={`font-semibold ${getTitleClass()}`}>
          {title}
        </AlertTitle>
        <AlertDescription className={`mt-1 ${getDescriptionClass()}`}>
          {description}
          {nextSteps.length > 0 && (
            <div className="mt-3">
              <p className="font-medium mb-2">Next steps:</p>
              <div className="flex flex-wrap gap-2">
                {nextSteps.map((step, index) => (
                  <Button
                    key={index}
                    variant={step.primary ? 'default' : 'outline'}
                    size="sm"
                    onClick={step.action}
                    className="text-xs"
                  >
                    {step.label}
                    {!step.primary && <ArrowRight className="h-3 w-3 ml-1" />}
                  </Button>
                ))}
              </div>
            </div>
          )}
        </AlertDescription>
      </div>
      {variant === 'celebration' && (
        <Badge variant="outline" className="ml-2 text-gray-700 border-gray-300">
          🎉
        </Badge>
      )}
    </Alert>
  );
}

// Pre-configured success feedback for common operations
export const successTemplates = {
  adapterCreated: (adapterName: string, onTest?: () => void, onView?: () => void) => (
    <SuccessFeedback
      title="Adapter Created Successfully!"
      description={`Your new adapter "${adapterName}" is ready to use.`}
      variant="celebration"
      nextSteps={[
        { label: 'Test It Now', action: onTest, primary: true },
        { label: 'View All Adapters', action: onView }
      ]}
    />
  ),

  adapterLoaded: (adapterName: string, onChat?: () => void) => (
    <SuccessFeedback
      title="Adapter Loaded"
      description={`${adapterName} is now active and ready for inference.`}
      nextSteps={[
        { label: 'Start Chatting', action: onChat, primary: true }
      ]}
    />
  ),

  trainingComplete: (adapterName: string, onTest?: () => void, onDownload?: () => void) => (
    <SuccessFeedback
      title="Training Complete!"
      description={`Your adapter "${adapterName}" has been trained and is ready to use.`}
      variant="milestone"
      nextSteps={[
        { label: 'Test Adapter', action: onTest, primary: true },
        { label: 'Download .aos File', action: onDownload }
      ]}
    />
  ),

  bulkOperation: (operation: string, successCount: number, totalCount: number, onView?: () => void) => (
    <SuccessFeedback
      title={`${operation} Complete`}
      description={`Successfully processed ${successCount} of ${totalCount} items.`}
      nextSteps={onView ? [{ label: 'View Results', action: onView }] : []}
    />
  )
};

export default SuccessFeedback;
