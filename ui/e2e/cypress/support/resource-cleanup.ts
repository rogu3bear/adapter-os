// Resource Cleanup Utilities for Cypress Backend API Tests
// Tracks created resources and provides cleanup functionality

interface CreatedResource {
  type: string;
  id: string;
  endpoint: string;
  method: string;
  createdAt: number;
}

class ResourceTracker {
  private resources: CreatedResource[] = [];

  /**
   * Track a created resource for cleanup
   */
  track(type: string, id: string, endpoint: string, method: string = 'DELETE'): void {
    this.resources.push({
      type,
      id,
      endpoint,
      method,
      createdAt: Date.now(),
    });
  }

  /**
   * Get all tracked resources
   */
  getAll(): CreatedResource[] {
    return [...this.resources];
  }

  /**
   * Get resources by type
   */
  getByType(type: string): CreatedResource[] {
    return this.resources.filter(r => r.type === type);
  }

  /**
   * Clear all tracked resources
   */
  clear(): void {
    this.resources = [];
  }

  /**
   * Remove a resource from tracking (after successful cleanup)
   */
  untrack(id: string): void {
    this.resources = this.resources.filter(r => r.id !== id);
  }
}

// Global resource tracker instance
let resourceTracker: ResourceTracker | null = null;

/**
 * Get or create the resource tracker instance
 */
function getResourceTracker(): ResourceTracker {
  if (!resourceTracker) {
    resourceTracker = new ResourceTracker();
  }
  return resourceTracker;
}

/**
 * Track a created resource for cleanup
 */
export function trackResource(
  type: string,
  id: string,
  endpoint: string,
  method: string = 'DELETE'
): void {
  getResourceTracker().track(type, id, endpoint, method);
}

/**
 * Clean up all tracked resources
 * Attempts to delete resources in reverse order of creation
 */
export function cleanupTrackedResources(): Cypress.Chainable<void> {
  const tracker = getResourceTracker();
  const resources = tracker.getAll();
  
  if (resources.length === 0) {
    return cy.wrap(undefined);
  }

  cy.log(`Cleaning up ${resources.length} tracked resources`);

  // Clean up in reverse order (most recent first)
  const sortedResources = [...resources].reverse();
  
  return cy.wrap(sortedResources).each((resource: CreatedResource) => {
    cy.apiRequest({
      method: resource.method,
      url: resource.endpoint,
      failOnStatusCode: false, // Don't fail if resource already deleted
    }).then((response) => {
      if (response.status === 200 || response.status === 204 || response.status === 404) {
        tracker.untrack(resource.id);
        cy.log(`Cleaned up ${resource.type} ${resource.id}`);
      } else {
        cy.log(`Failed to clean up ${resource.type} ${resource.id}: ${response.status}`);
      }
    });
  }).then(() => {
    tracker.clear();
  });
}

/**
 * Clear resource tracking without cleanup
 */
export function clearResourceTracking(): void {
  if (resourceTracker) {
    resourceTracker.clear();
  }
}

/**
 * Get tracked resources count
 */
export function getTrackedResourceCount(): number {
  return getResourceTracker().getAll().length;
}

