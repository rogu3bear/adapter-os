/**
 * Load markdown documentation from the docs/ folder
 * 
 * In development, this fetches from a local endpoint that serves the docs.
 * In production, docs should be bundled or served via API.
 */
import { logger, toError } from './logger';

export async function loadDocumentation(path: string): Promise<string> {
  try {
    // Try fetching from /api/docs endpoint (if available)
    // Otherwise, fetch from public/docs or use a fallback
    const response = await fetch(`/api/docs/${path}`);
    
    if (response.ok) {
      return await response.text();
    }
    
    // Fallback: try direct public folder access
    const publicResponse = await fetch(`/docs/${path}`);
    if (publicResponse.ok) {
      return await publicResponse.text();
    }
    
    const notFoundError = new Error(`Documentation not found: ${path}`);
    logger.error('Documentation not found', {
      component: 'doc-loader',
      operation: 'loadDocumentation',
      path,
      apiStatus: response.status,
      publicStatus: publicResponse.status,
    }, notFoundError);
    throw notFoundError;
  } catch (error) {
    // Log error before re-throwing
    const wrappedError = new Error(`Failed to load documentation: ${path}. ${error instanceof Error ? error.message : 'Unknown error'}`);
    logger.error('Failed to load documentation', {
      component: 'doc-loader',
      operation: 'loadDocumentation',
      path,
    }, toError(error));
    throw wrappedError;
  }
}

/**
 * Extract table of contents from markdown content
 */
export interface TocItem {
  level: number;
  title: string;
  id: string;
}

export function extractTableOfContents(content: string): TocItem[] {
  const toc: TocItem[] = [];
  const lines = content.split('\n');
  const headingRegex = /^(#{1,6})\s+(.+)$/;
  
  lines.forEach((line, index) => {
    const match = line.match(headingRegex);
    if (match) {
      const level = match[1].length;
      const title = match[2].trim();
      // Generate a simple ID from the title
      const id = title
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/^-|-$/g, '');
      
      toc.push({ level, title, id });
    }
  });
  
  return toc;
}

