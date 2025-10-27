import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Alert, AlertDescription } from './ui/alert';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { useLocation, useNavigate } from 'react-router-dom';
import { useAuth } from '@/layout/LayoutProvider';
import { getRoleGuidance } from '@/data/role-guidance';
import { BookOpen, ArrowRight, Lightbulb } from 'lucide-react';
import type { UserRole } from '@/api/types';

interface PageGuidance {
  title: string;
  tips: string[];
  relatedPages: Array<{
    label: string;
    route: string;
    description: string;
  }>;
}

const pageGuidanceMap: Record<string, Record<UserRole, PageGuidance>> = {
  '/training': {
    Operator: {
      title: 'Training Adapters',
      tips: [
        'Start with a template to get going quickly',
        'Monitor training metrics in real-time',
        'Save checkpoints regularly for long training runs'
      ],
      relatedPages: [
        { label: 'Test & Validate', route: '/testing', description: 'Test your trained adapter' },
        { label: 'Deploy & Manage', route: '/adapters', description: 'Deploy after validation' }
      ]
    },
    Admin: {
      title: 'Training Management',
      tips: [
        'Review training resource allocation',
        'Monitor system capacity during training'
      ],
      relatedPages: [
        { label: 'System Health', route: '/monitoring', description: 'Check resource usage' }
      ]
    },
    SRE: {
      title: 'Training Operations',
      tips: [
        'Monitor training job resource consumption',
        'Set up alerts for training failures'
      ],
      relatedPages: [
        { label: 'System Health', route: '/monitoring', description: 'Monitor resources' }
      ]
    },
    Compliance: { title: '', tips: [], relatedPages: [] },
    Auditor: { title: '', tips: [], relatedPages: [] },
    Viewer: { title: '', tips: [], relatedPages: [] }
  },
  '/testing': {
    Operator: {
      title: 'Testing & Validation',
      tips: [
        'Run golden baseline comparisons before promoting',
        'Check epsilon metrics for numerical accuracy',
        'Validate on diverse test cases'
      ],
      relatedPages: [
        { label: 'Compare Baselines', route: '/golden', description: 'Compare with golden runs' },
        { label: 'Promote', route: '/promotion', description: 'Promote after passing tests' }
      ]
    },
    Admin: { title: '', tips: [], relatedPages: [] },
    SRE: { title: '', tips: [], relatedPages: [] },
    Compliance: { title: '', tips: [], relatedPages: [] },
    Auditor: { title: '', tips: [], relatedPages: [] },
    Viewer: { title: '', tips: [], relatedPages: [] }
  },
  '/policies': {
    Admin: {
      title: 'Policy Management',
      tips: [
        'Review policy packs regularly',
        'Sign policies after review',
        'Compare policies before updating'
      ],
      relatedPages: [
        { label: 'Telemetry', route: '/telemetry', description: 'View policy enforcement logs' },
        { label: 'Audit Trails', route: '/audit', description: 'Review policy changes' }
      ]
    },
    Compliance: {
      title: 'Compliance Review',
      tips: [
        'Verify all 20 policy packs are compliant',
        'Export attestations for audit',
        'Monitor policy violations'
      ],
      relatedPages: [
        { label: 'Audit Trails', route: '/audit', description: 'Review policy audit trail' },
        { label: 'Telemetry', route: '/telemetry', description: 'Export compliance data' }
      ]
    },
    Auditor: {
      title: 'Policy Audit',
      tips: [
        'Verify policy signatures',
        'Review policy change history'
      ],
      relatedPages: [
        { label: 'Audit Trails', route: '/audit', description: 'Full audit history' }
      ]
    },
    Operator: { title: '', tips: [], relatedPages: [] },
    SRE: { title: '', tips: [], relatedPages: [] },
    Viewer: { title: '', tips: [], relatedPages: [] }
  }
};

export function ContextualHelp() {
  const location = useLocation();
  const navigate = useNavigate();
  const { user } = useAuth();

  if (!user) return null;

  const roleGuidance = getRoleGuidance(user.role);
  const pageGuidance = pageGuidanceMap[location.pathname]?.[user.role];

  // Don't show if no relevant guidance
  if (!pageGuidance || (pageGuidance.tips.length === 0 && pageGuidance.relatedPages.length === 0)) {
    return null;
  }

  return (
    <Card className="border-blue-200 bg-blue-50/50">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Lightbulb className="h-5 w-5 text-blue-600" />
          {pageGuidance.title}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* Tips */}
        {pageGuidance.tips.length > 0 && (
          <div className="space-y-2">
            <p className="text-sm font-medium text-muted-foreground">Quick Tips:</p>
            <ul className="space-y-1">
              {pageGuidance.tips.map((tip, idx) => (
                <li key={idx} className="flex items-start gap-2 text-sm">
                  <span className="text-blue-600 mt-0.5">•</span>
                  <span>{tip}</span>
                </li>
              ))}
            </ul>
          </div>
        )}

        {/* Related Pages */}
        {pageGuidance.relatedPages.length > 0 && (
          <div className="space-y-2">
            <p className="text-sm font-medium text-muted-foreground">Next Steps:</p>
            <div className="space-y-2">
              {pageGuidance.relatedPages.map((page) => (
                <Button
                  key={page.route}
                  variant="ghost"
                  size="sm"
                  className="w-full justify-start h-auto py-2"
                  onClick={() => navigate(page.route)}
                >
                  <div className="flex-1 text-left">
                    <div className="font-medium text-sm">{page.label}</div>
                    <div className="text-xs text-muted-foreground">{page.description}</div>
                  </div>
                  <ArrowRight className="h-4 w-4 flex-shrink-0" />
                </Button>
              ))}
            </div>
          </div>
        )}

        {/* Role-specific tip */}
        {roleGuidance && roleGuidance.tips.length > 0 && (
          <Alert>
            <BookOpen className="h-4 w-4" />
            <AlertDescription className="text-sm">
              <span className="font-medium">Role Tip: </span>
              {roleGuidance.tips[Math.floor(Math.random() * roleGuidance.tips.length)]}
            </AlertDescription>
          </Alert>
        )}
      </CardContent>
    </Card>
  );
}

