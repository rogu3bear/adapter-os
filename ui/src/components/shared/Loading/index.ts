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

// Spinner components
export {
  LoadingSpinner,
  spinnerVariants,
  type LoadingSpinnerProps,
} from "./LoadingSpinner";

// Overlay components
export {
  LoadingOverlay,
  overlayVariants,
  type LoadingOverlayProps,
} from "./LoadingOverlay";

// Skeleton components
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

// Card skeleton
export {
  CardSkeleton,
  CardSkeletonGrid,
  cardSkeletonVariants,
  type CardSkeletonProps,
  type CardSkeletonGridProps,
} from "./CardSkeleton";

// Page loader
export {
  PageLoader,
  LogoLoader,
  pageLoaderVariants,
  type PageLoaderProps,
  type LogoLoaderProps,
} from "./PageLoader";

// Inline loader
export {
  InlineLoader,
  ButtonLoader,
  DotsLoader,
  inlineLoaderVariants,
  type InlineLoaderProps,
  type ButtonLoaderProps,
  type DotsLoaderProps,
} from "./InlineLoader";
