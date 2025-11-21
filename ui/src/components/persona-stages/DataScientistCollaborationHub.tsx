import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import { Button } from '../ui/button';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '../ui/table';
import { Textarea } from '../ui/textarea';
import { Input } from '../ui/input';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '../ui/dialog';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import {
  Users,
  MessageSquare,
  Download,
  FileCode,
  Plus,
  Send,
  Clock,
  User,
  CheckCircle,
  Activity,
  AlertTriangle,
} from 'lucide-react';
import { logger } from '../../utils/logger';

// Shared experiment interface
interface SharedExperiment {
  id: string;
  name: string;
  owner: string;
  status: 'running' | 'completed' | 'failed';
  sharedWith: string[];
  createdAt: string;
  updatedAt: string;
  metrics?: {
    accuracy?: number;
    loss?: number;
  };
}

// Comment/annotation interface
interface ExperimentComment {
  id: string;
  experimentId: string;
  author: string;
  content: string;
  createdAt: string;
  highlightedRange?: {
    start: number;
    end: number;
  };
}

// Mock shared experiments
const mockExperiments: SharedExperiment[] = [
  {
    id: 'exp-001',
    name: 'Code Completion v3',
    owner: 'alice@team.com',
    status: 'completed',
    sharedWith: ['bob@team.com', 'charlie@team.com'],
    createdAt: '2025-01-15T10:00:00Z',
    updatedAt: '2025-01-16T14:30:00Z',
    metrics: { accuracy: 0.923, loss: 0.234 },
  },
  {
    id: 'exp-002',
    name: 'Bug Detection Model',
    owner: 'bob@team.com',
    status: 'running',
    sharedWith: ['alice@team.com'],
    createdAt: '2025-01-16T09:00:00Z',
    updatedAt: '2025-01-16T15:00:00Z',
    metrics: { accuracy: 0.891, loss: 0.312 },
  },
  {
    id: 'exp-003',
    name: 'Documentation Generator',
    owner: 'charlie@team.com',
    status: 'failed',
    sharedWith: ['alice@team.com', 'bob@team.com'],
    createdAt: '2025-01-14T11:00:00Z',
    updatedAt: '2025-01-14T16:45:00Z',
    metrics: { accuracy: 0.756, loss: 0.567 },
  },
];

// Mock comments
const mockComments: ExperimentComment[] = [
  {
    id: 'comment-001',
    experimentId: 'exp-001',
    author: 'bob@team.com',
    content: 'Great results! The accuracy improvement over the baseline is impressive.',
    createdAt: '2025-01-16T10:30:00Z',
  },
  {
    id: 'comment-002',
    experimentId: 'exp-001',
    author: 'charlie@team.com',
    content: 'Consider increasing batch size for faster convergence. Also, check the learning rate warmup.',
    createdAt: '2025-01-16T11:15:00Z',
  },
  {
    id: 'comment-003',
    experimentId: 'exp-002',
    author: 'alice@team.com',
    content: 'The validation loss seems to plateau after epoch 15. Might need early stopping.',
    createdAt: '2025-01-16T14:00:00Z',
  },
];

export default function DataScientistCollaborationHub() {
  const [experiments] = useState<SharedExperiment[]>(mockExperiments);
  const [comments, setComments] = useState<ExperimentComment[]>(mockComments);
  const [selectedExperiment, setSelectedExperiment] = useState<string | null>(null);
  const [newComment, setNewComment] = useState('');
  const [isShareDialogOpen, setIsShareDialogOpen] = useState(false);
  const [shareEmail, setShareEmail] = useState('');

  const getStatusBadge = (status: SharedExperiment['status']) => {
    switch (status) {
      case 'running':
        return (
          <Badge variant="outline" className="bg-blue-50 text-blue-700 border-blue-200">
            <Activity className="h-3 w-3 mr-1 animate-pulse" />
            Running
          </Badge>
        );
      case 'completed':
        return (
          <Badge variant="outline" className="bg-green-50 text-green-700 border-green-200">
            <CheckCircle className="h-3 w-3 mr-1" />
            Completed
          </Badge>
        );
      case 'failed':
        return (
          <Badge variant="outline" className="bg-red-50 text-red-700 border-red-200">
            <AlertTriangle className="h-3 w-3 mr-1" />
            Failed
          </Badge>
        );
      default:
        return <Badge variant="outline">{status}</Badge>;
    }
  };

  const formatDate = (dateStr: string) => {
    return new Date(dateStr).toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  };

  const getExperimentComments = (experimentId: string) => {
    return comments.filter((c) => c.experimentId === experimentId);
  };

  const handleAddComment = () => {
    if (!selectedExperiment || !newComment.trim()) return;

    const comment: ExperimentComment = {
      id: `comment-${Date.now()}`,
      experimentId: selectedExperiment,
      author: 'you@team.com',
      content: newComment.trim(),
      createdAt: new Date().toISOString(),
    };

    setComments([...comments, comment]);
    setNewComment('');

    logger.info('Comment added', {
      component: 'DataScientistCollaborationHub',
      experimentId: selectedExperiment,
    });
  };

  const handleShareExperiment = () => {
    if (!selectedExperiment || !shareEmail.trim()) return;

    logger.info('Experiment shared', {
      component: 'DataScientistCollaborationHub',
      experimentId: selectedExperiment,
      sharedWith: shareEmail,
    });

    setIsShareDialogOpen(false);
    setShareEmail('');
  };

  const handleExportToNotebook = (experimentId: string) => {
    const experiment = experiments.find((e) => e.id === experimentId);
    if (!experiment) return;

    // Generate Jupyter notebook format
    const notebook = {
      nbformat: 4,
      nbformat_minor: 5,
      metadata: {
        kernelspec: {
          display_name: 'Python 3',
          language: 'python',
          name: 'python3',
        },
      },
      cells: [
        {
          cell_type: 'markdown',
          metadata: {},
          source: [`# Experiment: ${experiment.name}\n`, `\nOwner: ${experiment.owner}\n`, `Created: ${experiment.createdAt}\n`],
        },
        {
          cell_type: 'code',
          metadata: {},
          source: [
            '# Experiment Configuration\n',
            `experiment_id = "${experiment.id}"\n`,
            `accuracy = ${experiment.metrics?.accuracy || 'None'}\n`,
            `loss = ${experiment.metrics?.loss || 'None'}\n`,
          ],
          outputs: [],
          execution_count: null,
        },
        {
          cell_type: 'markdown',
          metadata: {},
          source: ['## Comments and Annotations\n'],
        },
        ...getExperimentComments(experimentId).map((comment) => ({
          cell_type: 'markdown',
          metadata: {},
          source: [`**${comment.author}** (${formatDate(comment.createdAt)}):\n`, `> ${comment.content}\n`],
        })),
      ],
    };

    const blob = new Blob([JSON.stringify(notebook, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${experiment.name.replace(/\s+/g, '_')}.ipynb`;
    a.click();
    URL.revokeObjectURL(url);

    logger.info('Experiment exported to notebook', {
      component: 'DataScientistCollaborationHub',
      experimentId,
    });
  };

  const selectedExp = experiments.find((e) => e.id === selectedExperiment);

  return (
    <div className="space-y-6 p-4">
      <Card>
        <CardHeader className="flex flex-row items-center justify-between">
          <div>
            <CardTitle className="flex items-center gap-2">
              <Users className="h-5 w-5" />
              Collaboration Hub
            </CardTitle>
            <p className="text-sm text-muted-foreground mt-1">
              Share experiments and collaborate with your team
            </p>
          </div>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
            {/* Experiment List */}
            <div>
              <h3 className="text-sm font-medium mb-3">Shared Experiments</h3>
              <div className="border rounded-lg overflow-hidden">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Name</TableHead>
                      <TableHead>Owner</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Updated</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {experiments.map((exp) => (
                      <TableRow
                        key={exp.id}
                        className={`cursor-pointer ${selectedExperiment === exp.id ? 'bg-muted' : ''}`}
                        onClick={() => setSelectedExperiment(exp.id)}
                      >
                        <TableCell className="font-medium">{exp.name}</TableCell>
                        <TableCell className="text-sm text-muted-foreground">
                          {exp.owner.split('@')[0]}
                        </TableCell>
                        <TableCell>{getStatusBadge(exp.status)}</TableCell>
                        <TableCell className="text-sm text-muted-foreground">
                          {formatDate(exp.updatedAt)}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </div>
            </div>

            {/* Experiment Details & Comments */}
            <div>
              {selectedExp ? (
                <Tabs defaultValue="comments">
                  <TabsList className="mb-4">
                    <TabsTrigger value="comments">
                      <MessageSquare className="h-4 w-4 mr-2" />
                      Comments
                    </TabsTrigger>
                    <TabsTrigger value="details">
                      <FileCode className="h-4 w-4 mr-2" />
                      Details
                    </TabsTrigger>
                  </TabsList>

                  <TabsContent value="comments">
                    <Card>
                      <CardHeader className="pb-3">
                        <div className="flex items-center justify-between">
                          <CardTitle className="text-base">{selectedExp.name}</CardTitle>
                          <div className="flex gap-2">
                            <Dialog open={isShareDialogOpen} onOpenChange={setIsShareDialogOpen}>
                              <DialogTrigger asChild>
                                <Button variant="outline" size="sm">
                                  <Plus className="h-4 w-4 mr-1" />
                                  Share
                                </Button>
                              </DialogTrigger>
                              <DialogContent>
                                <DialogHeader>
                                  <DialogTitle>Share Experiment</DialogTitle>
                                </DialogHeader>
                                <div className="space-y-4 pt-4">
                                  <Input
                                    placeholder="email@team.com"
                                    value={shareEmail}
                                    onChange={(e) => setShareEmail(e.target.value)}
                                  />
                                  <Button className="w-full" onClick={handleShareExperiment}>
                                    Share
                                  </Button>
                                </div>
                              </DialogContent>
                            </Dialog>
                            <Button
                              variant="outline"
                              size="sm"
                              onClick={() => handleExportToNotebook(selectedExp.id)}
                            >
                              <Download className="h-4 w-4 mr-1" />
                              Export
                            </Button>
                          </div>
                        </div>
                      </CardHeader>
                      <CardContent>
                        <div className="space-y-4 max-h-64 overflow-y-auto mb-4">
                          {getExperimentComments(selectedExp.id).length > 0 ? (
                            getExperimentComments(selectedExp.id).map((comment) => (
                              <div key={comment.id} className="border-l-2 border-primary/30 pl-3 py-1">
                                <div className="flex items-center gap-2 text-xs text-muted-foreground mb-1">
                                  <User className="h-3 w-3" />
                                  <span>{comment.author.split('@')[0]}</span>
                                  <Clock className="h-3 w-3 ml-2" />
                                  <span>{formatDate(comment.createdAt)}</span>
                                </div>
                                <p className="text-sm">{comment.content}</p>
                              </div>
                            ))
                          ) : (
                            <p className="text-sm text-muted-foreground text-center py-4">
                              No comments yet. Be the first to add one!
                            </p>
                          )}
                        </div>

                        <div className="flex gap-2">
                          <Textarea
                            placeholder="Add a comment or annotation..."
                            value={newComment}
                            onChange={(e) => setNewComment(e.target.value)}
                            className="resize-none"
                            rows={2}
                          />
                          <Button
                            size="icon"
                            onClick={handleAddComment}
                            disabled={!newComment.trim()}
                          >
                            <Send className="h-4 w-4" />
                          </Button>
                        </div>
                      </CardContent>
                    </Card>
                  </TabsContent>

                  <TabsContent value="details">
                    <Card>
                      <CardContent className="pt-4">
                        <Table>
                          <TableBody>
                            <TableRow>
                              <TableCell className="font-medium">Owner</TableCell>
                              <TableCell>{selectedExp.owner}</TableCell>
                            </TableRow>
                            <TableRow>
                              <TableCell className="font-medium">Status</TableCell>
                              <TableCell>{getStatusBadge(selectedExp.status)}</TableCell>
                            </TableRow>
                            <TableRow>
                              <TableCell className="font-medium">Shared With</TableCell>
                              <TableCell>
                                <div className="flex flex-wrap gap-1">
                                  {selectedExp.sharedWith.map((email) => (
                                    <Badge key={email} variant="secondary" className="text-xs">
                                      {email.split('@')[0]}
                                    </Badge>
                                  ))}
                                </div>
                              </TableCell>
                            </TableRow>
                            <TableRow>
                              <TableCell className="font-medium">Accuracy</TableCell>
                              <TableCell className="font-mono">
                                {selectedExp.metrics?.accuracy
                                  ? `${(selectedExp.metrics.accuracy * 100).toFixed(1)}%`
                                  : '-'}
                              </TableCell>
                            </TableRow>
                            <TableRow>
                              <TableCell className="font-medium">Loss</TableCell>
                              <TableCell className="font-mono">
                                {selectedExp.metrics?.loss?.toFixed(4) || '-'}
                              </TableCell>
                            </TableRow>
                            <TableRow>
                              <TableCell className="font-medium">Created</TableCell>
                              <TableCell>{formatDate(selectedExp.createdAt)}</TableCell>
                            </TableRow>
                            <TableRow>
                              <TableCell className="font-medium">Last Updated</TableCell>
                              <TableCell>{formatDate(selectedExp.updatedAt)}</TableCell>
                            </TableRow>
                          </TableBody>
                        </Table>
                      </CardContent>
                    </Card>
                  </TabsContent>
                </Tabs>
              ) : (
                <div className="border rounded-lg p-8 text-center text-muted-foreground">
                  <Users className="h-12 w-12 mx-auto mb-4 opacity-50" />
                  <p>Select an experiment to view details and collaborate</p>
                </div>
              )}
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
