import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Textarea } from '@/components/ui/textarea';
import { Input } from '@/components/ui/input';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
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
  Loader2,
} from 'lucide-react';
import { logger } from '@/utils/logger';
import { useWorkspaces } from '@/hooks/workspace/useWorkspaces';
import apiClient from '@/api/client';
import type { WorkspaceResource, Message } from '@/api/types';

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

/**
 * Map WorkspaceResource to SharedExperiment
 * Resources of type 'training_job' or 'experiment' are considered experiments
 */
const mapResourceToExperiment = (resource: WorkspaceResource): SharedExperiment => {
  return {
    id: resource.id,
    name: resource.resource_name || `Resource ${resource.resource_id}`,
    owner: resource.shared_by || 'unknown',
    status: 'completed', // Default status, can be enhanced with metadata
    sharedWith: [], // Can be enhanced by fetching workspace members
    createdAt: resource.shared_at || new Date().toISOString(),
    updatedAt: resource.shared_at || new Date().toISOString(),
    metrics: {}, // Can be enhanced with resource metadata
  };
};

/**
 * Map Message to ExperimentComment
 */
const mapMessageToComment = (message: Message, experimentId: string): ExperimentComment => {
  return {
    id: message.id,
    experimentId,
    author: message.from_user_display_name || message.from || 'unknown',
    content: message.content || message.body,
    createdAt: message.created_at || message.timestamp,
  };
};

export default function DataScientistCollaborationHub() {
  const { userWorkspaces, isLoading: workspacesLoading, error: workspacesError } = useWorkspaces();
  const [experiments, setExperiments] = useState<SharedExperiment[]>([]);
  const [comments, setComments] = useState<ExperimentComment[]>([]);
  const [selectedExperiment, setSelectedExperiment] = useState<string | null>(null);
  const [newComment, setNewComment] = useState('');
  const [isShareDialogOpen, setIsShareDialogOpen] = useState(false);
  const [shareEmail, setShareEmail] = useState('');
  const [loadingResources, setLoadingResources] = useState(false);
  const [loadingComments, setLoadingComments] = useState(false);
  const [resourcesError, setResourcesError] = useState<string | null>(null);
  const [commentsError, setCommentsError] = useState<string | null>(null);

  // Fetch workspace resources (experiments)
  useEffect(() => {
    const fetchExperiments = async () => {
      if (userWorkspaces.length === 0) return;

      setLoadingResources(true);
      setResourcesError(null);

      try {
        // Fetch resources from all user workspaces
        const resourcePromises = userWorkspaces.map((workspace) =>
          apiClient.listWorkspaceResources(workspace.id)
        );
        const allResources = await Promise.all(resourcePromises);
        const flatResources = allResources.flat();

        // Filter for experiment-related resources and map to SharedExperiment
        const experimentResources = flatResources.filter(
          (resource) =>
            resource.resource_type === 'training_job' ||
            resource.resource_type === 'experiment' ||
            resource.resource_type === 'adapter'
        );
        const mappedExperiments = experimentResources.map(mapResourceToExperiment);

        setExperiments(mappedExperiments);

        logger.info('Experiments loaded', {
          component: 'DataScientistCollaborationHub',
          count: mappedExperiments.length,
        });
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : 'Failed to load experiments';
        setResourcesError(errorMessage);
        logger.error('Failed to fetch experiments', {
          component: 'DataScientistCollaborationHub',
        }, err instanceof Error ? err : new Error(String(err)));
      } finally {
        setLoadingResources(false);
      }
    };

    fetchExperiments();
  }, [userWorkspaces]);

  // Fetch workspace messages (comments) for selected experiment
  useEffect(() => {
    const fetchComments = async () => {
      if (!selectedExperiment || userWorkspaces.length === 0) return;

      setLoadingComments(true);
      setCommentsError(null);

      try {
        // Fetch messages from all user workspaces
        const messagePromises = userWorkspaces.map((workspace) =>
          apiClient.listWorkspaceMessages(workspace.id)
        );
        const allMessages = await Promise.all(messagePromises);
        const flatMessages = allMessages.flat();

        // Map messages to comments for the selected experiment
        const mappedComments = flatMessages.map((message) =>
          mapMessageToComment(message, selectedExperiment)
        );

        setComments(mappedComments);

        logger.info('Comments loaded', {
          component: 'DataScientistCollaborationHub',
          experimentId: selectedExperiment,
          count: mappedComments.length,
        });
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : 'Failed to load comments';
        setCommentsError(errorMessage);
        logger.error('Failed to fetch comments', {
          component: 'DataScientistCollaborationHub',
          experimentId: selectedExperiment,
        }, err instanceof Error ? err : new Error(String(err)));
      } finally {
        setLoadingComments(false);
      }
    };

    fetchComments();
  }, [selectedExperiment, userWorkspaces]);

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

  const handleAddComment = async () => {
    if (!selectedExperiment || !newComment.trim() || userWorkspaces.length === 0) return;

    try {
      // Use the first workspace for creating messages
      const workspace = userWorkspaces[0];
      const newMessage = await apiClient.createMessage(workspace.id, {
        content: newComment.trim(),
        subject: `Comment on experiment ${selectedExperiment}`,
      });

      const comment = mapMessageToComment(newMessage, selectedExperiment);
      setComments([...comments, comment]);
      setNewComment('');

      logger.info('Comment added', {
        component: 'DataScientistCollaborationHub',
        experimentId: selectedExperiment,
      });
    } catch (err) {
      logger.error('Failed to add comment', {
        component: 'DataScientistCollaborationHub',
        experimentId: selectedExperiment,
      }, err instanceof Error ? err : new Error(String(err)));
    }
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
  const isLoading = workspacesLoading || loadingResources;

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
          {/* Show workspace error */}
          {workspacesError && (
            <div className="bg-red-50 border border-red-200 text-red-700 px-4 py-3 rounded mb-4">
              <p className="text-sm font-medium">Failed to load workspaces</p>
              <p className="text-sm">{typeof workspacesError === 'string' ? workspacesError : workspacesError.message}</p>
            </div>
          )}

          {/* Show resources error */}
          {resourcesError && (
            <div className="bg-red-50 border border-red-200 text-red-700 px-4 py-3 rounded mb-4">
              <p className="text-sm font-medium">Failed to load experiments</p>
              <p className="text-sm">{resourcesError}</p>
            </div>
          )}

          <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
            {/* Experiment List */}
            <div>
              <h3 className="text-sm font-medium mb-3">Shared Experiments</h3>
              <div className="border rounded-lg overflow-hidden">
                {isLoading ? (
                  <div className="flex items-center justify-center py-8">
                    <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                    <span className="ml-2 text-sm text-muted-foreground">Loading experiments...</span>
                  </div>
                ) : experiments.length === 0 ? (
                  <div className="text-center py-8 text-muted-foreground">
                    <Users className="h-12 w-12 mx-auto mb-4 opacity-50" />
                    <p className="text-sm">No shared experiments found</p>
                    <p className="text-xs mt-1">Create and share experiments to collaborate with your team</p>
                  </div>
                ) : (
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
                )}
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
                        {/* Show comments error */}
                        {commentsError && (
                          <div className="bg-red-50 border border-red-200 text-red-700 px-3 py-2 rounded mb-4">
                            <p className="text-xs font-medium">Failed to load comments</p>
                            <p className="text-xs">{commentsError}</p>
                          </div>
                        )}

                        {loadingComments ? (
                          <div className="flex items-center justify-center py-8">
                            <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
                            <span className="ml-2 text-sm text-muted-foreground">Loading comments...</span>
                          </div>
                        ) : (
                          <>
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
                                disabled={loadingComments}
                              />
                              <Button
                                size="icon"
                                onClick={handleAddComment}
                                disabled={!newComment.trim() || loadingComments}
                                aria-label="Send comment"
                              >
                                <Send className="h-4 w-4" />
                              </Button>
                            </div>
                          </>
                        )}
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
