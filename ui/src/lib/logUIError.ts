import { captureException } from '@/stores/errorStore';
import { logger, toError } from '@/utils/logger';

export type UIErrorContext = {
  scope: 'global' | 'section' | 'page' | 'chat' | 'modal' | string;
  route?: string;
  pageKey?: string;
  component?: string;
};

export function logUIError(error: unknown, context: UIErrorContext) {
  const normalizedError = toError(error);
  const component = context.component ?? 'ui';

  logger.error('UI error', { ...context, component }, normalizedError);

  captureException(normalizedError, {
    component,
    operation: context.scope,
    extra: {
      route: context.route,
      pageKey: context.pageKey,
      scope: context.scope,
    },
  });
}

