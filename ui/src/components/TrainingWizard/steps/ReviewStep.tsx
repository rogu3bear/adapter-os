import React from 'react';
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Settings, FileText, Database, Zap, Folder } from 'lucide-react';
import { useTrainingWizardContext } from '@/components/TrainingWizard/context';

export function ReviewStep() {
  const { state } = useTrainingWizardContext();

  return (
    <div className="space-y-4">
      <Alert>
        <Settings className="h-4 w-4" />
        <AlertDescription>
          Review your configuration before starting training. This process may take several hours depending on the dataset size and hardware.
        </AlertDescription>
      </Alert>

      <Accordion type="multiple" defaultValue={['basic']} className="w-full">
        <AccordionItem value="basic">
          <AccordionTrigger>
            <div className="flex items-center gap-2">
              <FileText className="h-4 w-4" />
              Basic Information
            </div>
          </AccordionTrigger>
          <AccordionContent>
            <div className="grid grid-cols-2 gap-4 text-sm pt-2">
              <div>
                <p className="font-medium">Category</p>
                <p className="text-muted-foreground capitalize">{state.category}</p>
              </div>
              <div>
                <p className="font-medium">Name</p>
                <p className="text-muted-foreground">{state.name}</p>
              </div>
              <div>
                <p className="font-medium">Scope</p>
                <p className="text-muted-foreground capitalize">{state.scope}</p>
              </div>
              <div>
                <p className="font-medium">Description</p>
                <p className="text-muted-foreground">{state.description || 'No description'}</p>
              </div>
            </div>
          </AccordionContent>
        </AccordionItem>

        <AccordionItem value="data-source">
          <AccordionTrigger>
            <div className="flex items-center gap-2">
              <Database className="h-4 w-4" />
              Data Source
            </div>
          </AccordionTrigger>
          <AccordionContent>
            <div className="space-y-2 text-sm pt-2">
              <div>
                <p className="font-medium">Type</p>
                <p className="text-muted-foreground capitalize">{state.dataSourceType}</p>
              </div>
              {state.dataSourceType === 'directory' && state.directoryRoot && (
                <div>
                  <p className="font-medium">Directory Path</p>
                  <p className="text-xs text-muted-foreground font-mono">
                    {state.directoryRoot}{state.directoryPath ? `/${state.directoryPath}` : ''}
                  </p>
                </div>
              )}
              {state.dataSourceType === 'template' && state.templateId && (
                <div>
                  <p className="font-medium">Template ID</p>
                  <p className="text-muted-foreground">{state.templateId}</p>
                </div>
              )}
              {state.dataSourceType === 'repository' && state.repositoryId && (
                <div>
                  <p className="font-medium">Repository ID</p>
                  <p className="text-muted-foreground">{state.repositoryId}</p>
                </div>
              )}
              {state.datasetPath && (
                <div>
                  <p className="font-medium">Dataset Path</p>
                  <p className="text-muted-foreground font-mono text-xs">{state.datasetPath}</p>
                </div>
              )}
            </div>
          </AccordionContent>
        </AccordionItem>

        <AccordionItem value="category-config">
          <AccordionTrigger>
            <div className="flex items-center gap-2">
              <Settings className="h-4 w-4" />
              Category Configuration
            </div>
          </AccordionTrigger>
          <AccordionContent>
            <div className="space-y-2 text-sm pt-2">
              {state.category === 'code' && state.language && (
                <div>
                  <p className="font-medium">Language</p>
                  <Badge>{state.language}</Badge>
                  {state.symbolTargets && state.symbolTargets.length > 0 && (
                    <div className="mt-2">
                      <p className="font-medium">Symbol Targets</p>
                      <div className="flex flex-wrap gap-1 mt-1">
                        {state.symbolTargets.map((target) => (
                          <Badge key={target} variant="outline">{target}</Badge>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              )}
              {state.category === 'framework' && (
                <div className="space-y-2">
                  {state.frameworkId && (
                    <div>
                      <p className="font-medium">Framework</p>
                      <Badge>{state.frameworkId} {state.frameworkVersion || ''}</Badge>
                    </div>
                  )}
                  {state.apiPatterns && state.apiPatterns.length > 0 && (
                    <div>
                      <p className="font-medium">API Patterns</p>
                      <div className="flex flex-wrap gap-1 mt-1">
                        {state.apiPatterns.map((pattern) => (
                          <Badge key={pattern} variant="outline">{pattern}</Badge>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              )}
              {state.category === 'codebase' && (
                <div className="space-y-2">
                  {state.repoScope && (
                    <div>
                      <p className="font-medium">Repository Scope</p>
                      <p className="text-muted-foreground">{state.repoScope}</p>
                    </div>
                  )}
                  {state.filePatterns && state.filePatterns.length > 0 && (
                    <div>
                      <p className="font-medium">File Patterns</p>
                      <div className="flex flex-wrap gap-1 mt-1">
                        {state.filePatterns.map((pattern) => (
                          <Badge key={pattern} variant="outline">{pattern}</Badge>
                        ))}
                      </div>
                    </div>
                  )}
                  {state.excludePatterns && state.excludePatterns.length > 0 && (
                    <div>
                      <p className="font-medium">Exclude Patterns</p>
                      <div className="flex flex-wrap gap-1 mt-1">
                        {state.excludePatterns.map((pattern) => (
                          <Badge key={pattern} variant="outline">{pattern}</Badge>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              )}
              {state.category === 'ephemeral' && (
                <div className="space-y-2">
                  {state.ttlSeconds && (
                    <div>
                      <p className="font-medium">TTL</p>
                      <p className="text-muted-foreground">{state.ttlSeconds} seconds</p>
                    </div>
                  )}
                  {state.contextWindow && (
                    <div>
                      <p className="font-medium">Context Window</p>
                      <p className="text-muted-foreground">{state.contextWindow} tokens</p>
                    </div>
                  )}
                </div>
              )}
            </div>
          </AccordionContent>
        </AccordionItem>

        <AccordionItem value="training-params">
          <AccordionTrigger>
            <div className="flex items-center gap-2">
              <Zap className="h-4 w-4" />
              Training Parameters
            </div>
          </AccordionTrigger>
          <AccordionContent>
            <div className="grid grid-cols-2 gap-4 text-sm pt-2">
              <div>
                <p className="font-medium">Rank</p>
                <p className="text-muted-foreground">{state.rank}</p>
              </div>
              <div>
                <p className="font-medium">Alpha</p>
                <p className="text-muted-foreground">{state.alpha}</p>
              </div>
              <div>
                <p className="font-medium">Epochs</p>
                <p className="text-muted-foreground">{state.epochs}</p>
              </div>
              <div>
                <p className="font-medium">Learning Rate</p>
                <p className="text-muted-foreground">{state.learningRate}</p>
              </div>
              <div>
                <p className="font-medium">Batch Size</p>
                <p className="text-muted-foreground">{state.batchSize}</p>
              </div>
              {state.warmupSteps && (
                <div>
                  <p className="font-medium">Warmup Steps</p>
                  <p className="text-muted-foreground">{state.warmupSteps}</p>
                </div>
              )}
              {state.maxSeqLength && (
                <div>
                  <p className="font-medium">Max Sequence Length</p>
                  <p className="text-muted-foreground">{state.maxSeqLength}</p>
                </div>
              )}
            </div>
            <div className="mt-4">
              <p className="font-medium text-sm">LoRA Targets ({state.targets.length})</p>
              <div className="flex flex-wrap gap-1 mt-2">
                {state.targets.map((target) => (
                  <Badge key={target} variant="outline">{target}</Badge>
                ))}
              </div>
            </div>
          </AccordionContent>
        </AccordionItem>

        <AccordionItem value="packaging">
          <AccordionTrigger>
            <div className="flex items-center gap-2">
              <Folder className="h-4 w-4" />
              Packaging & Registration
            </div>
          </AccordionTrigger>
          <AccordionContent>
            <div className="space-y-2 text-sm pt-2">
              <div className="flex items-center gap-2">
                <p className="font-medium">Package After Training:</p>
                <Badge variant={state.packageAfter ? 'default' : 'outline'}>
                  {state.packageAfter ? 'Yes' : 'No'}
                </Badge>
              </div>
              <div className="flex items-center gap-2">
                <p className="font-medium">Register After Packaging:</p>
                <Badge variant={state.registerAfter ? 'default' : 'outline'}>
                  {state.registerAfter ? 'Yes' : 'No'}
                </Badge>
              </div>
              {state.registerAfter && (
                <div className="flex items-center gap-2 ml-4">
                  <p className="font-medium">Create Stack:</p>
                  <Badge variant={state.createStack !== false ? 'default' : 'outline'}>
                    {state.createStack !== false ? 'Yes' : 'No'}
                  </Badge>
                  {state.createStack !== false && (
                    <span className="text-xs text-muted-foreground">(not set as default)</span>
                  )}
                </div>
              )}
              {state.adaptersRoot && (
                <div>
                  <p className="font-medium">Adapters Root</p>
                  <p className="text-muted-foreground font-mono text-xs">{state.adaptersRoot}</p>
                </div>
              )}
              {state.adapterId && (
                <div>
                  <p className="font-medium">Adapter ID</p>
                  <p className="text-muted-foreground">{state.adapterId}</p>
                </div>
              )}
              {state.tier && (
                <div>
                  <p className="font-medium">Tier</p>
                  <p className="text-muted-foreground">{state.tier}</p>
                </div>
              )}
            </div>
          </AccordionContent>
        </AccordionItem>
      </Accordion>

      <Card>
        <CardHeader>
          <CardTitle>Configuration Summary</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div>
              <p className="font-medium">Category</p>
              <p className="text-muted-foreground capitalize">{state.category}</p>
            </div>
            <div>
              <p className="font-medium">Name</p>
              <p className="text-muted-foreground">{state.name}</p>
            </div>
            <div>
              <p className="font-medium">Scope</p>
              <p className="text-muted-foreground capitalize">{state.scope}</p>
            </div>
            <div>
              <p className="font-medium">Data Source</p>
              <p className="text-muted-foreground capitalize">{state.dataSourceType}</p>
            </div>
            <div>
              <p className="font-medium">Rank</p>
              <p className="text-muted-foreground">{state.rank}</p>
            </div>
            <div>
              <p className="font-medium">Epochs</p>
              <p className="text-muted-foreground">{state.epochs}</p>
            </div>
            <div>
              <p className="font-medium">Learning Rate</p>
              <p className="text-muted-foreground">{state.learningRate}</p>
            </div>
            <div>
              <p className="font-medium">Batch Size</p>
              <p className="text-muted-foreground">{state.batchSize}</p>
            </div>
          </div>

          {state.category === 'code' && state.language && (
            <div>
              <p className="font-medium text-sm">Language</p>
              <Badge>{state.language}</Badge>
            </div>
          )}

          {state.category === 'framework' && state.frameworkId && (
            <div>
              <p className="font-medium text-sm">Framework</p>
              <Badge>{state.frameworkId} {state.frameworkVersion}</Badge>
            </div>
          )}

          <div>
            <p className="font-medium text-sm">LoRA Targets ({state.targets.length})</p>
            <div className="flex flex-wrap gap-1 mt-1">
              {state.targets.map((target) => (
                <Badge key={target} variant="outline">{target}</Badge>
              ))}
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
