/**
 * Helper to attach stable data-testid attributes.
 *
 * Usage:
 *   <Button {...testId('my-button')}>Click</Button>
 */
export const testId = (value: string) => ({ 'data-testid': value });
