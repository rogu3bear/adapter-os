import React from 'react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { TERMS } from '@/constants/terminology';
import { formatBytes } from '@/lib/formatters';
import { AlertTriangle, FileText, FolderOpen, Check, Loader2 } from 'lucide-react';
import { FILE_VALIDATION } from '@/components/TrainingWizard/constants';
import { useTrainingWizardContext } from '@/components/TrainingWizard/context';

export function SimpleDatasetStep() {
  const {
    state,
    updateState,
    datasets,
    simpleDatasetMode,
    setSimpleDatasetMode,
    handleUploadFilesSelect,
    uploadFiles,
    uploadError,
    datasetName,
    setDatasetName,
    handleCreateAndValidateDataset,
    validationResult,
    createdDatasetId,
    handleOpenDatasetTools,
    createStatus,
    // Document/collection mode
    documents,
    collections,
    selectedDocumentId,
    setSelectedDocumentId,
    selectedCollectionId,
    setSelectedCollectionId,
    conversionStatus,
    conversionError,
    handleConvertToDataset,
  } = useTrainingWizardContext();

  // Filter to indexed documents only
  const indexedDocuments = documents.filter(d => d.status === 'indexed');

  return (
    <div className="space-y-4">
      <p className="text-sm text-muted-foreground">
        Choose your training data source: use an existing collection, create from a document, or upload new files.
      </p>

      <div className="flex flex-wrap gap-2">
        <Button
          type="button"
          variant={simpleDatasetMode === 'existing' ? 'default' : 'outline'}
          onClick={() => setSimpleDatasetMode('existing')}
          disabled={conversionStatus === 'converting'}
        >
          Existing collection
        </Button>
        <Button
          type="button"
          variant={simpleDatasetMode === 'document' ? 'default' : 'outline'}
          onClick={() => {
            setSimpleDatasetMode('document');
            setSelectedCollectionId(null);
          }}
          disabled={conversionStatus === 'converting'}
        >
          <FileText className="h-4 w-4 mr-1" />
          From document
        </Button>
        <Button
          type="button"
          variant={simpleDatasetMode === 'collection' ? 'default' : 'outline'}
          onClick={() => {
            setSimpleDatasetMode('collection');
            setSelectedDocumentId(null);
          }}
          disabled={conversionStatus === 'converting'}
        >
          <FolderOpen className="h-4 w-4 mr-1" />
          From collection
        </Button>
        <Button
          type="button"
          variant={simpleDatasetMode === 'upload' ? 'default' : 'outline'}
          onClick={() => setSimpleDatasetMode('upload')}
          disabled={conversionStatus === 'converting'}
        >
          Upload new
        </Button>
      </div>

      {simpleDatasetMode === 'existing' && (
        <div className="space-y-2">
          <Label htmlFor="dataset">{TERMS.selectDataset}</Label>
          <Select value={state.datasetId} onValueChange={(value) => {
            updateState({
              datasetId: value,
              dataSourceType: 'dataset',
              category: state.category || 'codebase',
              name: state.name || `adapter-${Date.now()}`,
              scope: state.scope || 'tenant',
              packageAfter: true,
              registerAfter: true,
              adaptersRoot: './adapters',
              tier: 'warm',
              targets: state.targets.length > 0 ? state.targets : ['q_proj', 'v_proj'],
            });
          }}>
            <SelectTrigger id="dataset">
              <SelectValue placeholder="Choose a collection..." />
            </SelectTrigger>
            <SelectContent>
              {datasets.length === 0 ? (
                <SelectItem value="__empty__" disabled>{TERMS.noDatasets}</SelectItem>
              ) : (
                datasets.map((dataset) => (
                  <SelectItem key={dataset.id} value={dataset.id}>
                    <div className="flex items-center justify-between gap-2 w-full">
                      <div className="flex items-center gap-2">
                        <span>{dataset.name}</span>
                        <Badge variant="outline" className="text-xs">
                          {dataset.validation_status}
                        </Badge>
                      </div>
                      <span className="text-xs text-muted-foreground ml-auto">
                        {dataset.file_count} files • {formatBytes(dataset.total_size_bytes)}
                      </span>
                    </div>
                  </SelectItem>
                ))
              )}
            </SelectContent>
          </Select>
          {datasets.length === 0 && (
            <p className="text-xs text-muted-foreground">
              {TERMS.noDatasetsDescription}. Switch to "Upload new documents" to create one.
            </p>
          )}
          {state.datasetId && (() => {
            const selectedDataset = datasets.find(d => d.id === state.datasetId);
            if (selectedDataset && selectedDataset.validation_status !== 'valid') {
              return (
                <Alert variant="destructive">
                  <AlertTriangle className="h-4 w-4" />
                  <AlertDescription>
                    <p className="font-medium">
                      Collection "{selectedDataset.name}" must be validated before training.
                    </p>
                    <p className="text-sm mt-1">
                      Current status: {selectedDataset.validation_status}.
                    </p>
                    {selectedDataset.validation_errors && (
                      <p className="text-sm mt-2 font-mono">
                        {selectedDataset.validation_errors}
                      </p>
                    )}
                    <p className="text-sm mt-2">
                      Please validate from the Document Collections page.
                    </p>
                  </AlertDescription>
                </Alert>
              );
            }
            return null;
          })()}
        </div>
      )}

      {simpleDatasetMode === 'document' && (
        <div className="space-y-3 border rounded-lg p-4">
          <div className="space-y-2">
            <Label htmlFor="document-select">Select a document</Label>
            <Select
              value={selectedDocumentId || ''}
              onValueChange={(value) => setSelectedDocumentId(value)}
            >
              <SelectTrigger id="document-select">
                <SelectValue placeholder="Choose a document..." />
              </SelectTrigger>
              <SelectContent>
                {indexedDocuments.length === 0 ? (
                  <SelectItem value="__empty__" disabled>
                    No indexed documents available
                  </SelectItem>
                ) : (
                  indexedDocuments.map((doc) => (
                    <SelectItem key={doc.document_id} value={doc.document_id}>
                      <div className="flex items-center gap-2">
                        <FileText className="h-4 w-4 text-muted-foreground" />
                        <span>{doc.name}</span>
                        <Badge variant="outline" className="text-xs">
                          {doc.status}
                        </Badge>
                      </div>
                    </SelectItem>
                  ))
                )}
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground">
              Only indexed documents can be converted to training data.
            </p>
          </div>

          {conversionError && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>{conversionError}</AlertDescription>
            </Alert>
          )}

          {conversionStatus === 'done' && createdDatasetId && (
            <Alert>
              <Check className="h-4 w-4 text-green-600" />
              <AlertDescription className="text-green-700">
                Dataset created successfully! You can proceed to the next step.
              </AlertDescription>
            </Alert>
          )}

          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              onClick={handleConvertToDataset}
              disabled={!selectedDocumentId || conversionStatus === 'converting'}
            >
              {conversionStatus === 'converting' ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  Converting...
                </>
              ) : (
                'Convert to dataset'
              )}
            </Button>
            {createdDatasetId && conversionStatus === 'done' && (
              <Button
                type="button"
                variant="outline"
                onClick={() => handleOpenDatasetTools(createdDatasetId)}
              >
                View dataset
              </Button>
            )}
          </div>
        </div>
      )}

      {simpleDatasetMode === 'collection' && (
        <div className="space-y-3 border rounded-lg p-4">
          <div className="space-y-2">
            <Label htmlFor="collection-select">Select a document collection</Label>
            <Select
              value={selectedCollectionId || ''}
              onValueChange={(value) => setSelectedCollectionId(value)}
            >
              <SelectTrigger id="collection-select">
                <SelectValue placeholder="Choose a collection..." />
              </SelectTrigger>
              <SelectContent>
                {collections.length === 0 ? (
                  <SelectItem value="__empty__" disabled>
                    No collections available
                  </SelectItem>
                ) : (
                  collections.map((col) => (
                    <SelectItem key={col.collection_id} value={col.collection_id}>
                      <div className="flex items-center gap-2">
                        <FolderOpen className="h-4 w-4 text-muted-foreground" />
                        <span>{col.name}</span>
                        {col.document_count !== undefined && (
                          <span className="text-xs text-muted-foreground">
                            ({col.document_count} docs)
                          </span>
                        )}
                      </div>
                    </SelectItem>
                  ))
                )}
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground">
              All indexed documents in the collection will be converted to training data.
            </p>
          </div>

          {conversionError && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>{conversionError}</AlertDescription>
            </Alert>
          )}

          {conversionStatus === 'done' && createdDatasetId && (
            <Alert>
              <Check className="h-4 w-4 text-green-600" />
              <AlertDescription className="text-green-700">
                Dataset created successfully! You can proceed to the next step.
              </AlertDescription>
            </Alert>
          )}

          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              onClick={handleConvertToDataset}
              disabled={!selectedCollectionId || conversionStatus === 'converting'}
            >
              {conversionStatus === 'converting' ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  Converting...
                </>
              ) : (
                'Convert to dataset'
              )}
            </Button>
            {createdDatasetId && conversionStatus === 'done' && (
              <Button
                type="button"
                variant="outline"
                onClick={() => handleOpenDatasetTools(createdDatasetId)}
              >
                View dataset
              </Button>
            )}
          </div>
        </div>
      )}

      {simpleDatasetMode === 'upload' && (
        <div className="space-y-3 border rounded-lg p-4">
          <div className="space-y-1">
            <Label htmlFor="wizard-dataset-name">{TERMS.datasetName}</Label>
            <Input
              id="wizard-dataset-name"
              value={datasetName}
              onChange={(e) => setDatasetName(e.target.value)}
              placeholder="my-collection"
            />
            <p className="text-xs text-muted-foreground">
              Optional. Defaults to your adapter name or a generated value.
            </p>
          </div>

          <div className="space-y-2">
            <Label>Upload {TERMS.documents.toLowerCase()}</Label>
            <Input
              type="file"
              multiple
              accept={FILE_VALIDATION.allowedExtensions.join(',')}
              onChange={(e) => handleUploadFilesSelect(e.target.files)}
            />
            <p className="text-xs text-muted-foreground">
              Supported: {FILE_VALIDATION.allowedExtensions.join(', ')} • Max {FILE_VALIDATION.maxSize / (1024 * 1024)}MB each
            </p>
            {uploadFiles.length > 0 && (
              <div className="space-y-1 text-sm">
                <p className="font-medium">Selected ({uploadFiles.length}):</p>
                <ul className="list-disc list-inside text-muted-foreground max-h-24 overflow-auto">
                  {uploadFiles.map((file) => (
                    <li key={file.name}>{file.name}</li>
                  ))}
                </ul>
              </div>
            )}
          </div>

          {uploadError && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>{uploadError}</AlertDescription>
            </Alert>
          )}

          {validationResult && (
            <Card className="border-muted">
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Badge variant={validationResult.status === 'valid' ? 'outline' : 'destructive'}>
                    {validationResult.status}
                  </Badge>
                  <span>Validation result</span>
                </CardTitle>
                <CardContent className="space-y-2">
                  {validationResult.errors && validationResult.errors.length > 0 && (
                    <div className="text-sm text-red-600 space-y-1">
                      {validationResult.errors.map((err, idx) => <p key={idx}>{err}</p>)}
                    </div>
                  )}
                  {validationResult.warnings && validationResult.warnings.length > 0 && (
                    <div className="text-sm text-yellow-700 space-y-1">
                      {validationResult.warnings.map((warn, idx) => <p key={idx}>{warn}</p>)}
                    </div>
                  )}
                </CardContent>
              </CardHeader>
            </Card>
          )}

          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              onClick={handleCreateAndValidateDataset}
              disabled={createStatus !== 'idle'}
            >
              {createStatus === 'creating' && 'Uploading...'}
              {createStatus === 'validating' && TERMS.datasetValidating}
              {createStatus === 'idle' && 'Create & validate collection'}
            </Button>
            {createdDatasetId && (
              <Button
                type="button"
                variant="outline"
                onClick={() => handleOpenDatasetTools(createdDatasetId)}
              >
                Open in Collection Tools
              </Button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
