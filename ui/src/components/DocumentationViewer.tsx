import React, { useState, useEffect, useMemo, useCallback } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Card, CardContent } from './ui/card';
import { ScrollArea } from './ui/scroll-area';
import { Badge } from './ui/badge';
import { Search, BookOpen, ChevronRight, Loader2, AlertCircle } from 'lucide-react';
import { documentationIndex, type DocumentationEntry, searchDocumentation } from '@/data/documentation-index';
import { loadDocumentation, extractTableOfContents, type TocItem } from '@/utils/doc-loader';
import { cn } from './ui/utils';
import 'highlight.js/styles/github-dark.css';

function getDocumentationEntryById(docId?: string | null): DocumentationEntry | null {
  if (!docId) return null;
  return documentationIndex.find(
    (doc) => doc.id === docId || doc.slug === docId
  ) ?? null;
}

function extractHeadingText(children: React.ReactNode): string {
  return React.Children.toArray(children)
    .map((child) => {
      if (typeof child === 'string' || typeof child === 'number') {
        return String(child);
      }
      return '';
    })
    .join(' ')
    .trim();
}

function slugifyHeading(children: React.ReactNode): string {
  const value = extractHeadingText(children);
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
}

function createHeadingRenderer(Tag: keyof JSX.IntrinsicElements) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return ({ node, ...props }: any) => {
    const id = slugifyHeading(props.children);
    return <Tag id={id} {...props} />;
  };
}
interface DocumentationViewerProps {
  initialDocId?: string;
  onDocChange?: (docSlug: string) => void;
}

export function DocumentationViewer({ initialDocId, onDocChange }: DocumentationViewerProps) {
  const initialDoc = getDocumentationEntryById(initialDocId);
  const [selectedDoc, setSelectedDoc] = useState<DocumentationEntry | null>(initialDoc);
  const [content, setContent] = useState<string>('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [toc, setToc] = useState<TocItem[]>([]);
  const [activeHeading, setActiveHeading] = useState<string>('');

  const categories: Array<{ value: DocumentationEntry['category'] | 'all'; label: string }> = [
    { value: 'all', label: 'All' },
    { value: 'getting-started', label: 'Getting Started' },
    { value: 'architecture', label: 'Architecture' },
    { value: 'api', label: 'API Reference' },
    { value: 'guides', label: 'Guides' },
    { value: 'operations', label: 'Operations' },
    { value: 'development', label: 'Development' }
  ];

  const [selectedCategory, setSelectedCategory] = useState<DocumentationEntry['category'] | 'all'>('all');

  const filteredDocs = useMemo(() => {
    if (searchQuery.trim()) {
      return searchDocumentation(searchQuery);
    }
    
    if (selectedCategory === 'all') {
      return documentationIndex;
    }
    
    return documentationIndex.filter(doc => doc.category === selectedCategory);
  }, [searchQuery, selectedCategory]);

  const featuredDocs = useMemo(() => {
    return documentationIndex.filter(doc => doc.featured);
  }, []);

  useEffect(() => {
    if (initialDocId === undefined) {
      return;
    }
    const resolved = getDocumentationEntryById(initialDocId);
    if (resolved) {
      setSelectedDoc(resolved);
      setError(null);
      if (onDocChange && (resolved.slug ?? resolved.id) !== initialDocId) {
        onDocChange(resolved.slug ?? resolved.id);
      }
    } else if (initialDocId) {
      setSelectedDoc(null);
      setContent('');
      setToc([]);
      setError(`Documentation "${initialDocId}" is not indexed. Choose another guide from the list.`);
    }
  }, [initialDocId, onDocChange]);

  // Load documentation content
  useEffect(() => {
    if (!selectedDoc) {
      setContent('');
      setToc([]);
      return;
    }

    setLoading(true);
    setError(null);
    
    loadDocumentation(selectedDoc.path)
      .then((markdown) => {
        setContent(markdown);
        const extractedToc = extractTableOfContents(markdown);
        setToc(extractedToc);
        if (selectedDoc.anchor) {
          const anchorId = selectedDoc.anchor;
          setActiveHeading(anchorId);
          setTimeout(() => {
            scrollToHeading(anchorId);
          }, 200);
        } else {
          setActiveHeading('');
        }
      })
      .catch((err) => {
        setError(err instanceof Error ? err.message : 'Failed to load documentation');
        setContent('');
        setToc([]);
      })
      .finally(() => {
        setLoading(false);
      });
  }, [selectedDoc]);

  // Scroll to heading when clicking TOC
  const scrollToHeading = (id: string) => {
    const element = document.getElementById(id);
    if (element) {
      element.scrollIntoView({ behavior: 'smooth', block: 'start' });
      setActiveHeading(id);
    }
  };

  const handleSelectDoc = useCallback((doc: DocumentationEntry) => {
    setSelectedDoc(doc);
    setError(null);
    setActiveHeading(doc.anchor ?? '');
    onDocChange?.(doc.slug ?? doc.id);
  }, [onDocChange]);

  return (
    <div className="flex h-full gap-4">
      {/* Sidebar: Documentation List */}
      <div className="w-80 border-r flex flex-col">
        <div className="p-4 border-b space-y-4">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Search documentation..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-9"
            />
          </div>
          
          <div className="flex flex-wrap gap-2">
            {categories.map((cat) => (
              <Button
                key={cat.value}
                variant={selectedCategory === cat.value ? 'default' : 'outline'}
                size="sm"
                onClick={() => setSelectedCategory(cat.value)}
                className="text-xs"
              >
                {cat.label}
              </Button>
            ))}
          </div>
        </div>

        <ScrollArea className="flex-1">
          <div className="p-4 space-y-4">
            {searchQuery || selectedCategory !== 'all' ? (
              <div className="space-y-2">
                <h3 className="text-sm font-semibold text-muted-foreground">Search Results</h3>
                {filteredDocs.length === 0 ? (
                  <p className="text-sm text-muted-foreground">No documentation found</p>
                ) : (
                  filteredDocs.map((doc) => (
                    <button
                      key={doc.id}
                      onClick={() => handleSelectDoc(doc)}
                      className={cn(
                        "w-full text-left p-2 rounded-md hover:bg-accent transition-colors",
                        selectedDoc?.id === doc.id && "bg-accent"
                      )}
                    >
                      <div className="flex items-start justify-between gap-2">
                        <div className="flex-1 min-w-0">
                          <div className="font-medium text-sm truncate">{doc.title}</div>
                          <div className="text-xs text-muted-foreground line-clamp-2 mt-1">
                            {doc.description}
                          </div>
                        </div>
                        <ChevronRight className="h-4 w-4 text-muted-foreground flex-shrink-0 mt-1" />
                      </div>
                    </button>
                  ))
                )}
              </div>
            ) : (
              <>
                {featuredDocs.length > 0 && (
                  <div className="space-y-2">
                    <h3 className="text-sm font-semibold text-muted-foreground">Featured</h3>
                    {featuredDocs.map((doc) => (
                      <button
                        key={doc.id}
                        onClick={() => handleSelectDoc(doc)}
                        className={cn(
                          "w-full text-left p-2 rounded-md hover:bg-accent transition-colors",
                          selectedDoc?.id === doc.id && "bg-accent"
                        )}
                      >
                        <div className="flex items-start justify-between gap-2">
                          <div className="flex-1 min-w-0">
                            <div className="font-medium text-sm truncate">{doc.title}</div>
                            <div className="text-xs text-muted-foreground line-clamp-2 mt-1">
                              {doc.description}
                            </div>
                          </div>
                          <ChevronRight className="h-4 w-4 text-muted-foreground flex-shrink-0 mt-1" />
                        </div>
                      </button>
                    ))}
                  </div>
                )}
                
                {categories.filter(c => c.value !== 'all').map((category) => {
                  const docs = documentationIndex.filter(d => d.category === category.value);
                  if (docs.length === 0) return null;
                  
                  return (
                    <div key={category.value} className="space-y-2">
                      <h3 className="text-sm font-semibold text-muted-foreground">{category.label}</h3>
                      {docs.map((doc) => (
                        <button
                          key={doc.id}
                          onClick={() => handleSelectDoc(doc)}
                          className={cn(
                            "w-full text-left p-2 rounded-md hover:bg-accent transition-colors",
                            selectedDoc?.id === doc.id && "bg-accent"
                          )}
                        >
                          <div className="font-medium text-sm">{doc.title}</div>
                        </button>
                      ))}
                    </div>
                  );
                })}
              </>
            )}
          </div>
        </ScrollArea>
      </div>

      {/* Main Content Area */}
      <div className="flex-1 flex flex-col min-w-0">
        {selectedDoc ? (
          <>
            <div className="border-b p-4">
              <div className="flex items-start justify-between gap-4">
                <div className="flex-1 min-w-0">
                  <h1 className="text-2xl font-bold mb-2">{selectedDoc.title}</h1>
                  <p className="text-sm text-muted-foreground mb-2">{selectedDoc.description}</p>
                  <div className="flex items-center gap-2">
                    <Badge variant="secondary">{selectedDoc.category}</Badge>
                    <span className="text-xs text-muted-foreground">{selectedDoc.path}</span>
                  </div>
                </div>
              </div>
            </div>

            <div className="flex flex-1 gap-4 overflow-hidden">
              {/* Content */}
              <ScrollArea className="flex-1">
                <div className="p-6">
                  {loading ? (
                    <div className="flex items-center justify-center py-12">
                      <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                      <span className="ml-2 text-muted-foreground">Loading documentation...</span>
                    </div>
                  ) : error ? (
                    <Card className="border-destructive">
                      <CardContent className="p-6">
                        <div className="flex items-start gap-3 text-destructive">
                          <AlertCircle className="h-5 w-5 mt-1" />
                          <div>
                            <h3 className="font-semibold">Error loading documentation</h3>
                            <p className="text-sm mt-1">{error}</p>
                            <p className="text-xs text-muted-foreground mt-2">
                              Check your connection or choose another guide from the list. Files can also be served via <code>/api/docs/</code> or bundled in <code>public/docs/</code>.
                            </p>
                          </div>
                        </div>
                      </CardContent>
                    </Card>
                  ) : (
                    <div className="prose prose-sm dark:prose-invert max-w-none">
                      <ReactMarkdown
                        remarkPlugins={[remarkGfm]}
                        rehypePlugins={[rehypeHighlight]}
                        components={{
                          h1: createHeadingRenderer('h1'),
                          h2: createHeadingRenderer('h2'),
                          h3: createHeadingRenderer('h3'),
                          h4: createHeadingRenderer('h4'),
                          h5: createHeadingRenderer('h5'),
                          h6: createHeadingRenderer('h6'),
                        }}
                      >
                        {content}
                      </ReactMarkdown>
                    </div>
                  )}
                </div>
              </ScrollArea>

              {/* Table of Contents */}
              {toc.length > 0 && (
                <div className="w-64 border-l p-4">
                  <h3 className="text-sm font-semibold mb-3 flex items-center gap-2">
                    <BookOpen className="h-4 w-4" />
                    Contents
                  </h3>
                  <ScrollArea className="h-full">
                    <nav className="space-y-1">
                      {toc.map((item) => (
                        <button
                          key={item.id}
                          onClick={() => scrollToHeading(item.id)}
                          className={cn(
                            "w-full text-left text-sm py-1 px-2 rounded hover:bg-accent transition-colors",
                            activeHeading === item.id && "bg-accent font-medium",
                            item.level === 1 && "font-semibold",
                            item.level === 2 && "ml-2",
                            item.level === 3 && "ml-4",
                            item.level >= 4 && "ml-6"
                          )}
                        >
                          {item.title}
                        </button>
                      ))}
                    </nav>
                  </ScrollArea>
                </div>
              )}
            </div>
          </>
        ) : (
          <div className="flex-1 flex items-center justify-center">
            <div className="text-center max-w-md">
              <BookOpen className="h-12 w-12 text-muted-foreground mx-auto mb-4" />
              <h2 className="text-xl font-semibold mb-2">Select Documentation</h2>
              <p className="text-sm text-muted-foreground">
                Choose a documentation entry from the sidebar to view its contents
              </p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
