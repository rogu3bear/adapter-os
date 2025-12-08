import { captureException } from '@/stores/errorStore';
import { LogLevel, logger, toError } from '@/utils/logger';

export type UIErrorSeverity = 'info' | 'warning' | 'error';

export type UIErrorContext = {
  scope: 'global' | 'section' | 'page' | 'chat' | 'modal' | string;
  route?: string;
  pageKey?: string;
  component?: string;
  severity?: UIErrorSeverity;
  userMessageKey?: string;
};

export function logUIError(error: unknown, context: UIErrorContext) {
  const normalizedError = toError(error);
  const component = context.component ?? 'ui';
  const severity: UIErrorSeverity = context.severity ?? 'error';
  const level: LogLevel =
    severity === 'warning' ? LogLevel.WARN :
    severity === 'info' ? LogLevel.INFO :
    LogLevel.ERROR;
  const message =
    severity === 'warning' ? 'UI warning' :
    severity === 'info' ? 'UI info' :
    'UI error';

  logger.log(level, message, { ...context, component, severity }, normalizedError);

  captureException(normalizedError, {
    component,
    operation: context.scope,
    extra: {
      route: context.route,
      pageKey: context.pageKey,
      scope: context.scope,
      severity,
      userMessageKey: context.userMessageKey,
    },
  });
}

export function logUIWarning(error: unknown, context: Omit<UIErrorContext, 'severity'>) {
  return logUIError(error, { ...context, severity: 'warning' });
}

export function logAuthEvent(message: string, context?: Record<string, unknown>) {
  logger.info(message, { component: 'auth-ui', ...context });
}

