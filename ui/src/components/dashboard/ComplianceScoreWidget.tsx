import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import { Progress } from '../ui/progress';
import { Shield, CheckCircle, AlertTriangle, XCircle } from 'lucide-react';
import { Button } from '../ui/button';
import { useNavigate } from 'react-router-dom';

interface PolicyPackStatus {
  name: string;
  compliant: boolean;
  violations: number;
}

export function ComplianceScoreWidget() {
  const navigate = useNavigate();

  // Mock compliance data - in production, fetch from API
  const overallScore = 98;
  const totalPacks = 20;
  const compliantPacks = 19;
  const violations = 2;

  const policyPacks: PolicyPackStatus[] = [
    { name: 'Egress Control', compliant: true, violations: 0 },
    { name: 'Determinism', compliant: true, violations: 0 },
    { name: 'Router Config', compliant: false, violations: 2 },
    { name: 'Evidence Rules', compliant: true, violations: 0 },
    { name: 'Telemetry', compliant: true, violations: 0 }
  ];

  const getScoreColor = (score: number) => {
    if (score >= 95) return 'text-green-600';
    if (score >= 80) return 'text-yellow-600';
    return 'text-red-600';
  };

  const getScoreBadge = (score: number) => {
    if (score >= 95) return 'Excellent';
    if (score >= 80) return 'Good';
    return 'Needs Attention';
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Shield className="h-5 w-5" />
            <span>Compliance Score</span>
          </div>
          <Badge variant={overallScore >= 95 ? 'default' : 'destructive'}>
            {getScoreBadge(overallScore)}
          </Badge>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* Overall Score */}
        <div className="text-center">
          <div className={`text-4xl font-bold ${getScoreColor(overallScore)}`}>
            {overallScore}%
          </div>
          <p className="text-sm text-muted-foreground mt-1">
            {compliantPacks}/{totalPacks} Policy Packs Compliant
          </p>
        </div>

        {/* Progress Ring or Bar */}
        <div>
          <Progress value={overallScore} className="h-2" />
        </div>

        {/* Policy Pack Summary */}
        <div className="space-y-2">
          <div className="flex items-center justify-between text-sm">
            <span className="text-muted-foreground">Policy Packs</span>
            {violations > 0 && (
              <span className="text-red-600 font-medium">{violations} violations</span>
            )}
          </div>
          <div className="space-y-1">
            {policyPacks.map((pack) => (
              <div key={pack.name} className="flex items-center justify-between text-sm p-2 rounded hover:bg-muted">
                <div className="flex items-center gap-2">
                  {pack.compliant ? (
                    <CheckCircle className="h-4 w-4 text-green-600" />
                  ) : (
                    <XCircle className="h-4 w-4 text-red-600" />
                  )}
                  <span>{pack.name}</span>
                </div>
                {pack.violations > 0 && (
                  <Badge variant="destructive" className="text-xs">
                    {pack.violations}
                  </Badge>
                )}
              </div>
            ))}
          </div>
        </div>

        {/* Action Button */}
        {violations > 0 && (
          <Button
            variant="outline"
            size="sm"
            className="w-full"
            onClick={() => navigate('/policies')}
          >
            <AlertTriangle className="h-4 w-4 mr-2" />
            Review Violations
          </Button>
        )}
      </CardContent>
    </Card>
  );
}

