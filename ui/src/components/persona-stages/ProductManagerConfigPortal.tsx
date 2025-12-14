import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Progress } from '@/components/ui/progress';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import {
  Flag,
  FlaskConical,
  Rocket,
  Plus,
  Pencil,
  Trash2,
  Calendar,
  Users,
  BarChart3,
  CheckCircle2,
  Clock,
  AlertCircle,
} from 'lucide-react';

interface FeatureFlag {
  id: string;
  key: string;
  name: string;
  description: string;
  enabled: boolean;
  rolloutPercentage: number;
  environment: 'development' | 'staging' | 'production';
  createdAt: string;
  updatedAt: string;
}

interface ABTest {
  id: string;
  name: string;
  description: string;
  status: 'draft' | 'running' | 'paused' | 'completed';
  variants: { name: string; weight: number }[];
  participants: number;
  startDate: string;
  endDate?: string;
}

interface Release {
  id: string;
  version: string;
  name: string;
  status: 'planned' | 'in-progress' | 'released' | 'rolled-back';
  releaseDate: string;
  features: string[];
  changelog: string;
}

const initialFeatureFlags: FeatureFlag[] = [
  {
    id: 'ff-1',
    key: 'dark_mode_v2',
    name: 'Dark Mode V2',
    description: 'Enhanced dark mode with improved contrast',
    enabled: true,
    rolloutPercentage: 100,
    environment: 'production',
    createdAt: '2025-01-10',
    updatedAt: '2025-01-15',
  },
  {
    id: 'ff-2',
    key: 'new_adapter_ui',
    name: 'New Adapter UI',
    description: 'Redesigned adapter management interface',
    enabled: true,
    rolloutPercentage: 50,
    environment: 'staging',
    createdAt: '2025-01-12',
    updatedAt: '2025-01-18',
  },
  {
    id: 'ff-4',
    key: 'batch_inference',
    name: 'Batch Inference',
    description: 'Enable batch inference processing',
    enabled: true,
    rolloutPercentage: 25,
    environment: 'production',
    createdAt: '2025-01-08',
    updatedAt: '2025-01-19',
  },
];

const initialABTests: ABTest[] = [
  {
    id: 'ab-1',
    name: 'Onboarding Flow V2',
    description: 'Testing new onboarding steps with guided tutorials',
    status: 'running',
    variants: [
      { name: 'Control', weight: 50 },
      { name: 'New Flow', weight: 50 },
    ],
    participants: 1234,
    startDate: '2025-01-15',
  },
  {
    id: 'ab-2',
    name: 'Dashboard Layout',
    description: 'Comparing card vs list layout for metrics',
    status: 'completed',
    variants: [
      { name: 'Card Layout', weight: 33 },
      { name: 'List Layout', weight: 33 },
      { name: 'Hybrid', weight: 34 },
    ],
    participants: 5678,
    startDate: '2025-01-01',
    endDate: '2025-01-14',
  },
  {
    id: 'ab-3',
    name: 'Pricing Page CTA',
    description: 'Testing different call-to-action button text',
    status: 'paused',
    variants: [
      { name: 'Get Started', weight: 50 },
      { name: 'Start Free Trial', weight: 50 },
    ],
    participants: 890,
    startDate: '2025-01-10',
  },
];

const initialReleases: Release[] = [
  {
    id: 'rel-1',
    version: 'v2.1.0',
    name: 'Performance Release',
    status: 'released',
    releaseDate: '2025-01-15',
    features: ['Optimized routing', 'Memory improvements', 'New telemetry'],
    changelog: 'Major performance improvements across all inference operations.',
  },
  {
    id: 'rel-2',
    version: 'v2.2.0',
    name: 'UI Refresh',
    status: 'in-progress',
    releaseDate: '2025-01-25',
    features: ['New dashboard', 'Dark mode v2', 'Accessibility improvements'],
    changelog: 'Complete UI overhaul with improved accessibility.',
  },
  {
    id: 'rel-3',
    version: 'v2.3.0',
    name: 'Enterprise Features',
    status: 'planned',
    releaseDate: '2025-02-10',
    features: ['SSO integration', 'Advanced RBAC', 'Audit logging'],
    changelog: 'Enterprise-focused features for large deployments.',
  },
];

export default function ProductManagerConfigPortal() {
  const [featureFlags, setFeatureFlags] = useState<FeatureFlag[]>(initialFeatureFlags);
  const [abTests] = useState<ABTest[]>(initialABTests);
  const [releases] = useState<Release[]>(initialReleases);
  const [newFlagDialogOpen, setNewFlagDialogOpen] = useState(false);
  const [newFlagName, setNewFlagName] = useState('');
  const [newFlagDescription, setNewFlagDescription] = useState('');
  const [newFlagEnvironment, setNewFlagEnvironment] = useState<'development' | 'staging' | 'production'>('development');

  const toggleFeatureFlag = (id: string) => {
    setFeatureFlags((prev) =>
      prev.map((flag) =>
        flag.id === id ? { ...flag, enabled: !flag.enabled, updatedAt: new Date().toISOString().split('T')[0] } : flag
      )
    );
  };

  const updateRolloutPercentage = (id: string, percentage: number) => {
    setFeatureFlags((prev) =>
      prev.map((flag) =>
        flag.id === id
          ? { ...flag, rolloutPercentage: percentage, updatedAt: new Date().toISOString().split('T')[0] }
          : flag
      )
    );
  };

  const deleteFeatureFlag = (id: string) => {
    setFeatureFlags((prev) => prev.filter((flag) => flag.id !== id));
  };

  const createFeatureFlag = () => {
    if (!newFlagName.trim()) return;
    const newFlag: FeatureFlag = {
      id: `ff-${Date.now()}`,
      key: newFlagName.toLowerCase().replace(/\s+/g, '_'),
      name: newFlagName,
      description: newFlagDescription,
      enabled: false,
      rolloutPercentage: 0,
      environment: newFlagEnvironment,
      createdAt: new Date().toISOString().split('T')[0],
      updatedAt: new Date().toISOString().split('T')[0],
    };
    setFeatureFlags((prev) => [...prev, newFlag]);
    setNewFlagName('');
    setNewFlagDescription('');
    setNewFlagEnvironment('development');
    setNewFlagDialogOpen(false);
  };

  const getEnvironmentBadgeVariant = (env: string) => {
    switch (env) {
      case 'production':
        return 'default';
      case 'staging':
        return 'secondary';
      default:
        return 'outline';
    }
  };

  const getStatusBadgeVariant = (status: string) => {
    switch (status) {
      case 'running':
        return 'default';
      case 'completed':
        return 'secondary';
      case 'paused':
        return 'outline';
      default:
        return 'outline';
    }
  };

  const getReleaseStatusIcon = (status: string) => {
    switch (status) {
      case 'released':
        return <CheckCircle2 className="h-4 w-4 text-green-500" />;
      case 'in-progress':
        return <Clock className="h-4 w-4 text-blue-500" />;
      case 'planned':
        return <Calendar className="h-4 w-4 text-gray-500" />;
      case 'rolled-back':
        return <AlertCircle className="h-4 w-4 text-red-500" />;
      default:
        return null;
    }
  };

  return (
    <div className="space-y-6 p-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">Configuration Portal</h1>
          <p className="text-sm text-muted-foreground">
            Manage feature flags, A/B tests, and releases
          </p>
        </div>
      </div>

      <Tabs defaultValue="feature-flags" className="space-y-4">
        <TabsList>
          <TabsTrigger value="feature-flags" className="gap-2">
            <Flag className="h-4 w-4" />
            Feature Flags
          </TabsTrigger>
          <TabsTrigger value="ab-tests" className="gap-2">
            <FlaskConical className="h-4 w-4" />
            A/B Tests
          </TabsTrigger>
          <TabsTrigger value="releases" className="gap-2">
            <Rocket className="h-4 w-4" />
            Releases
          </TabsTrigger>
        </TabsList>

        <TabsContent value="feature-flags" className="space-y-4">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between">
              <div>
                <CardTitle>Feature Flags</CardTitle>
                <CardDescription>Control feature rollouts across environments</CardDescription>
              </div>
              <Dialog open={newFlagDialogOpen} onOpenChange={setNewFlagDialogOpen}>
                <DialogTrigger asChild>
                  <Button size="sm" className="gap-2">
                    <Plus className="h-4 w-4" />
                    New Flag
                  </Button>
                </DialogTrigger>
                <DialogContent>
                  <DialogHeader>
                    <DialogTitle>Create Feature Flag</DialogTitle>
                    <DialogDescription>
                      Add a new feature flag to control feature rollout
                    </DialogDescription>
                  </DialogHeader>
                  <div className="space-y-4 py-4">
                    <div className="space-y-2">
                      <Label htmlFor="flag-name">Flag Name</Label>
                      <Input
                        id="flag-name"
                        placeholder="e.g., new_feature"
                        value={newFlagName}
                        onChange={(e) => setNewFlagName(e.target.value)}
                      />
                    </div>
                    <div className="space-y-2">
                      <Label htmlFor="flag-description">Description</Label>
                      <Input
                        id="flag-description"
                        placeholder="Describe the feature"
                        value={newFlagDescription}
                        onChange={(e) => setNewFlagDescription(e.target.value)}
                      />
                    </div>
                    <div className="space-y-2">
                      <Label htmlFor="flag-environment">Environment</Label>
                      <Select value={newFlagEnvironment} onValueChange={(v) => setNewFlagEnvironment(v as 'development' | 'staging' | 'production')}>
                        <SelectTrigger>
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="development">Development</SelectItem>
                          <SelectItem value="staging">Staging</SelectItem>
                          <SelectItem value="production">Production</SelectItem>
                        </SelectContent>
                      </Select>
                    </div>
                  </div>
                  <DialogFooter>
                    <Button variant="outline" onClick={() => setNewFlagDialogOpen(false)}>
                      Cancel
                    </Button>
                    <Button onClick={createFeatureFlag}>Create Flag</Button>
                  </DialogFooter>
                </DialogContent>
              </Dialog>
            </CardHeader>
            <CardContent>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Flag Name</TableHead>
                    <TableHead>Environment</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Rollout</TableHead>
                    <TableHead>Updated</TableHead>
                    <TableHead className="text-right">Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {featureFlags.map((flag) => (
                    <TableRow key={flag.id}>
                      <TableCell>
                        <div>
                          <div className="font-medium font-mono text-sm">{flag.name}</div>
                          <div className="text-xs text-muted-foreground">{flag.description}</div>
                        </div>
                      </TableCell>
                      <TableCell>
                        <Badge variant={getEnvironmentBadgeVariant(flag.environment)}>
                          {flag.environment}
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <Switch
                          checked={flag.enabled}
                          onCheckedChange={() => toggleFeatureFlag(flag.id)}
                        />
                      </TableCell>
                      <TableCell className="min-w-[150px]">
                        <div className="space-y-1">
                          <div className="flex items-center justify-between text-xs">
                            <span>{flag.rolloutPercentage}%</span>
                          </div>
                          <Progress value={flag.rolloutPercentage} className="h-2" />
                          <Input
                            type="range"
                            min="0"
                            max="100"
                            value={flag.rolloutPercentage}
                            onChange={(e) => updateRolloutPercentage(flag.id, parseInt(e.target.value))}
                            className="h-1 w-full"
                          />
                        </div>
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {flag.updatedAt}
                      </TableCell>
                      <TableCell className="text-right">
                        <div className="flex items-center justify-end gap-1">
                          <Button variant="ghost" size="icon" aria-label={`Edit ${flag.key}`}>
                            <Pencil className="h-4 w-4" />
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon"
                            onClick={() => deleteFeatureFlag(flag.id)}
                            aria-label={`Delete ${flag.key}`}
                          >
                            <Trash2 className="h-4 w-4" />
                          </Button>
                        </div>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="ab-tests" className="space-y-4">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between">
              <div>
                <CardTitle>A/B Tests</CardTitle>
                <CardDescription>Manage experiments and analyze results</CardDescription>
              </div>
              <Button size="sm" className="gap-2">
                <Plus className="h-4 w-4" />
                New Test
              </Button>
            </CardHeader>
            <CardContent>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Test Name</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Variants</TableHead>
                    <TableHead>Participants</TableHead>
                    <TableHead>Duration</TableHead>
                    <TableHead className="text-right">Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {abTests.map((test) => (
                    <TableRow key={test.id}>
                      <TableCell>
                        <div>
                          <div className="font-medium">{test.name}</div>
                          <div className="text-xs text-muted-foreground">{test.description}</div>
                        </div>
                      </TableCell>
                      <TableCell>
                        <Badge variant={getStatusBadgeVariant(test.status)}>
                          {test.status}
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <div className="space-y-1">
                          {test.variants.map((variant) => (
                            <div key={variant.name} className="text-xs">
                              <span className="font-medium">{variant.name}</span>
                              <span className="text-muted-foreground ml-1">({variant.weight}%)</span>
                            </div>
                          ))}
                        </div>
                      </TableCell>
                      <TableCell>
                        <div className="flex items-center gap-1">
                          <Users className="h-4 w-4 text-muted-foreground" />
                          <span>{test.participants.toLocaleString()}</span>
                        </div>
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        <div>{test.startDate}</div>
                        {test.endDate && <div className="text-xs">to {test.endDate}</div>}
                      </TableCell>
                      <TableCell className="text-right">
                        <div className="flex items-center justify-end gap-1">
                          <Button variant="ghost" size="icon" aria-label={`View analytics for ${test.name}`}>
                            <BarChart3 className="h-4 w-4" />
                          </Button>
                          <Button variant="ghost" size="icon" aria-label={`Edit ${test.name}`}>
                            <Pencil className="h-4 w-4" />
                          </Button>
                        </div>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="releases" className="space-y-4">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between">
              <div>
                <CardTitle>Release Management</CardTitle>
                <CardDescription>Track and manage product releases</CardDescription>
              </div>
              <Button size="sm" className="gap-2">
                <Plus className="h-4 w-4" />
                New Release
              </Button>
            </CardHeader>
            <CardContent>
              <div className="space-y-4">
                {releases.map((release) => (
                  <Card key={release.id} className="border">
                    <CardContent className="p-4">
                      <div className="flex items-start justify-between">
                        <div className="flex items-center gap-3">
                          {getReleaseStatusIcon(release.status)}
                          <div>
                            <div className="flex items-center gap-2">
                              <span className="font-bold">{release.version}</span>
                              <span className="text-muted-foreground">-</span>
                              <span className="font-medium">{release.name}</span>
                            </div>
                            <div className="text-sm text-muted-foreground mt-1">
                              {release.changelog}
                            </div>
                          </div>
                        </div>
                        <div className="text-right">
                          <Badge variant={release.status === 'released' ? 'default' : 'outline'}>
                            {release.status}
                          </Badge>
                          <div className="text-xs text-muted-foreground mt-1">
                            {release.releaseDate}
                          </div>
                        </div>
                      </div>
                      <div className="mt-3 flex flex-wrap gap-1">
                        {release.features.map((feature) => (
                          <Badge key={feature} variant="secondary" className="text-xs">
                            {feature}
                          </Badge>
                        ))}
                      </div>
                    </CardContent>
                  </Card>
                ))}
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
