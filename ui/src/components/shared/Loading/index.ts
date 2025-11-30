/**
 * Loading and Skeleton Components
 *
 * A comprehensive set of loading indicators and skeleton placeholders
 * for building consistent loading states across the application.
 *
 * @example
 * // Spinner
 * import { LoadingSpinner } from './Loading';
 * <LoadingSpinner size="lg" />
 *
 * @example
 * // Full page loader
 * import { PageLoader } from './Loading';
 * <PageLoader title="Loading application..." showProgress progress={45} />
 *
 * @example
 * // Table skeleton
 * import { TableSkeleton } from './Loading';
 * <TableSkeleton rows={10} columns={5} showCheckbox showActions />
 *
 * @example
 * // Button loading state
 * import { ButtonLoader } from './Loading';
 * <Button disabled={isLoading}>
 *   <ButtonLoader loading={isLoading} text="Saving...">
 *     Save Changes
 *   </ButtonLoader>
 * </Button>
 */

// Skeleton components (primary loading patterns)
export {
  Skeleton,
  SkeletonText,
  skeletonVariants,
  type SkeletonProps,
  type SkeletonTextProps,
} from "./Skeleton";

// Table skeleton
export {
  TableSkeleton,
  CompactTableSkeleton,
  type TableSkeletonProps,
} from "./TableSkeleton";
