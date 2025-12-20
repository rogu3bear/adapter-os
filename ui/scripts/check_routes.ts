import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import * as ts from 'typescript';

type EnvMode = 'development' | 'production';

type ComponentKind = 'component' | 'redirectTo' | 'redirectTelemetry' | 'other';

interface ParsedRoute {
  path: string;
  componentName?: string;
  componentKind: ComponentKind;
  redirectTarget?: string;
  navTitle?: string;
  navGroup?: string;
  roleVisibility?: string[];
  requiredRoles?: string[];
  requiredPermissions?: string[];
  modes?: string[];
}

const projectRoot = path.resolve(__dirname, '..');
const routesFile = path.join(projectRoot, 'src', 'config', 'routes.ts');
const manifestFile = path.join(projectRoot, 'src', 'config', 'routes_manifest.ts');
const allowedModes = new Set(['user', 'builder', 'audit']);

function main() {
  const sourceText = readFileSync(routesFile, 'utf8');
  const routesSource = ts.createSourceFile(routesFile, sourceText, ts.ScriptTarget.ESNext, true, ts.ScriptKind.TSX);

  const manifestText = readFileSync(manifestFile, 'utf8');
  const manifestSource = ts.createSourceFile(manifestFile, manifestText, ts.ScriptTarget.ESNext, true, ts.ScriptKind.TSX);

  const lazyImports = extractLazyImports(routesSource);
  const prodRoutes = extractRoutes(routesSource, false);
  const devRoutes = extractRoutes(routesSource, true);
  const primarySpine = extractPrimarySpine(manifestSource);

  const errors: string[] = [];
  errors.push(...validateModuleResolution(prodRoutes, lazyImports, 'production'));
  errors.push(...validateModuleResolution(devRoutes, lazyImports, 'development'));
  errors.push(...validateRedirectTargets(prodRoutes, 'production'));
  errors.push(...validateRedirectTargets(devRoutes, 'development'));
  errors.push(...validateRoleCoherence(prodRoutes, 'production'));
  errors.push(...validateRoleCoherence(devRoutes, 'development'));
  errors.push(...validateModeFiltering(prodRoutes, 'production'));
  errors.push(...validateModeFiltering(devRoutes, 'development'));
  errors.push(...validateDevRoutesExcluded(prodRoutes));
  errors.push(...validatePrimarySpine(primarySpine, prodRoutes, devRoutes));
  errors.push(...validateNoTypeCasts(sourceText));
  errors.push(...validateNoModalComponents(prodRoutes));
  errors.push(...validateNoModalComponents(devRoutes));
  errors.push(...validateComponentProps(lazyImports, prodRoutes));

  if (errors.length > 0) {
    console.error('Route validation failed:');
    for (const error of errors) {
      console.error(` - ${error}`);
    }
    process.exit(1);
  }

  console.log(
    `Route validation passed (prod routes: ${prodRoutes.length}, dev routes: ${devRoutes.length}, lazy modules: ${lazyImports.size})`,
  );
}

function extractLazyImports(source: ts.SourceFile): Map<string, string> {
  const map = new Map<string, string>();

  function visit(node: ts.Node) {
    if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name) && node.initializer) {
      const importPath = getLazyImportPath(node.initializer);
      if (importPath) {
        map.set(node.name.text, importPath);
      }
    }
    ts.forEachChild(node, visit);
  }

  visit(source);
  return map;
}

const ALLOWED_LAZY_FUNCTIONS = ['lazy', 'lazyWithRetry', 'lazyRouteable', 'lazyRouteableNamed'];

function getLazyImportPath(expr: ts.Expression): string | undefined {
  if (!ts.isCallExpression(expr)) {
    return undefined;
  }

  const callee = expr.expression;
  if (!ts.isIdentifier(callee) || !ALLOWED_LAZY_FUNCTIONS.includes(callee.text)) {
    return undefined;
  }

  const target = expr.arguments[0];
  if (!target || !ts.isArrowFunction(target)) {
    return undefined;
  }

  const body = target.body;
  if (ts.isBlock(body)) {
    const returnStmt = body.statements.find(ts.isReturnStatement);
    if (!returnStmt?.expression) {
      return undefined;
    }
    return findImportPath(returnStmt.expression);
  }

  if (ts.isCallExpression(body) || ts.isParenthesizedExpression(body)) {
    return findImportPath(body as ts.Expression);
  }

  return undefined;
}

function findImportPath(expr: ts.Expression): string | undefined {
  if (ts.isCallExpression(expr) && expr.expression.kind === ts.SyntaxKind.ImportKeyword) {
    const arg = expr.arguments[0];
    return ts.isStringLiteral(arg) ? arg.text : undefined;
  }

  if (ts.isCallExpression(expr) && ts.isPropertyAccessExpression(expr.expression)) {
    return findImportPath(expr.expression.expression);
  }

  if (ts.isParenthesizedExpression(expr)) {
    return findImportPath(expr.expression);
  }

  return undefined;
}

function extractRoutes(source: ts.SourceFile, isDev: boolean): ParsedRoute[] {
  let initializer: ts.Expression | undefined;

  source.forEachChild(node => {
    if (ts.isVariableStatement(node)) {
      node.declarationList.declarations.forEach(decl => {
        if (ts.isIdentifier(decl.name) && decl.name.text === 'routes' && decl.initializer) {
          initializer = decl.initializer;
        }
      });
    }
  });

  if (!initializer || !ts.isArrayLiteralExpression(initializer)) {
    throw new Error('Unable to locate routes array in routes.ts');
  }

  return flattenRouteArray(initializer, isDev);
}

function flattenRouteArray(arrayNode: ts.ArrayLiteralExpression, isDev: boolean): ParsedRoute[] {
  const routes: ParsedRoute[] = [];

  for (const element of arrayNode.elements) {
    if (ts.isSpreadElement(element)) {
      const spreadExpr = unwrapParens(element.expression);
      if (ts.isConditionalExpression(spreadExpr)) {
        const conditionValue = evaluateDevCondition(spreadExpr.condition, isDev);
        const branch = conditionValue ? spreadExpr.whenTrue : spreadExpr.whenFalse;
        routes.push(...extractFromExpression(branch, isDev));
        continue;
      }
      routes.push(...extractFromExpression(spreadExpr, isDev));
      continue;
    }

    if (ts.isObjectLiteralExpression(element)) {
      const route = extractRouteObject(element);
      if (route) {
        routes.push(route);
      }
    }
  }

  return routes;
}

function extractFromExpression(expr: ts.Expression, isDev: boolean): ParsedRoute[] {
  if (ts.isParenthesizedExpression(expr)) {
    return extractFromExpression(expr.expression, isDev);
  }
  if (ts.isArrayLiteralExpression(expr)) {
    return flattenRouteArray(expr, isDev);
  }
  return [];
}

function evaluateDevCondition(expr: ts.Expression, isDev: boolean): boolean {
  const text = expr
    .getText()
    .replace(/\s+/g, '')
    .replace(/^\(+|\)+$/g, '');

  if (text === 'import.meta.env.DEV') {
    return isDev;
  }
  if (text === '!import.meta.env.DEV') {
    return !isDev;
  }
  return false;
}

function unwrapParens(expr: ts.Expression): ts.Expression {
  let current = expr;
  while (ts.isParenthesizedExpression(current)) {
    current = current.expression;
  }
  return current;
}

function extractRouteObject(obj: ts.ObjectLiteralExpression): ParsedRoute | undefined {
  const route: ParsedRoute = { path: '', componentKind: 'other' };

  for (const prop of obj.properties) {
    if (!ts.isPropertyAssignment(prop)) {
      continue;
    }

    const key = getPropertyName(prop.name);
    const value = prop.initializer;

    switch (key) {
      case 'path':
        route.path = getString(value) ?? '';
        break;
      case 'component': {
        const componentInfo = getComponentInfo(value);
        route.componentName = componentInfo?.name;
        route.componentKind = componentInfo?.kind ?? 'other';
        route.redirectTarget = componentInfo?.target;
        break;
      }
      case 'navTitle':
        route.navTitle = getString(value);
        break;
      case 'navGroup':
        route.navGroup = getString(value);
        break;
      case 'roleVisibility':
        route.roleVisibility = getStringArray(value);
        break;
      case 'requiredRoles':
        route.requiredRoles = getStringArray(value);
        break;
      case 'requiredPermissions':
        route.requiredPermissions = getStringArray(value);
        break;
      case 'modes':
        route.modes = getModeArray(value);
        break;
      default:
        break;
    }
  }

  return route.path ? route : undefined;
}

function getComponentInfo(value: ts.Expression): { name?: string; kind: ComponentKind; target?: string } | undefined {
  if (ts.isIdentifier(value)) {
    return { name: value.text, kind: 'component' };
  }

  if (ts.isCallExpression(value) && ts.isIdentifier(value.expression)) {
    if (value.expression.text === 'redirectTo') {
      const target = value.arguments[0] ? getString(value.arguments[0]) : undefined;
      return { kind: 'redirectTo', target };
    }
    if (value.expression.text === 'redirectTelemetry') {
      return { kind: 'redirectTelemetry' };
    }
  }

  return undefined;
}

function getPropertyName(name: ts.PropertyName): string | undefined {
  if (ts.isIdentifier(name) || ts.isStringLiteral(name)) {
    return name.text;
  }
  return undefined;
}

function getString(expr: ts.Expression): string | undefined {
  if (ts.isStringLiteral(expr) || ts.isNoSubstitutionTemplateLiteral(expr)) {
    return expr.text;
  }
  return undefined;
}

function getStringArray(expr: ts.Expression): string[] | undefined {
  if (!ts.isArrayLiteralExpression(expr)) {
    return undefined;
  }

  const values: string[] = [];
  expr.elements.forEach(element => {
    if (ts.isStringLiteral(element) || ts.isNoSubstitutionTemplateLiteral(element)) {
      values.push(element.text);
    }
  });
  return values;
}

function getModeArray(expr: ts.Expression): string[] | undefined {
  if (!ts.isArrayLiteralExpression(expr)) {
    return undefined;
  }

  const values: string[] = [];
  expr.elements.forEach(element => {
    if (ts.isPropertyAccessExpression(element) && ts.isIdentifier(element.expression) && element.expression.text === 'UiMode') {
      values.push(element.name.text.toLowerCase());
    } else if (ts.isStringLiteral(element) || ts.isNoSubstitutionTemplateLiteral(element)) {
      values.push(element.text.toLowerCase());
    }
  });
  return values;
}

function validateModuleResolution(routes: ParsedRoute[], lazyImports: Map<string, string>, label: EnvMode): string[] {
  const errors: string[] = [];

  for (const route of routes) {
    if (!route.componentName) {
      continue;
    }
    const importPath = lazyImports.get(route.componentName);
    if (!importPath) {
      continue;
    }

    const resolved = resolveImportPath(importPath);
    if (!resolved) {
      errors.push(`[${label}] ${route.path} -> ${route.componentName} missing module (${importPath})`);
    }
  }

  return errors;
}

function resolveImportPath(importPath: string): string | undefined {
  const basePath = importPath.startsWith('@/') ? path.join(projectRoot, 'src', importPath.slice(2)) : path.resolve(projectRoot, importPath);
  const candidates = [
    basePath,
    `${basePath}.tsx`,
    `${basePath}.ts`,
    path.join(basePath, 'index.tsx'),
    path.join(basePath, 'index.ts'),
  ];

  return candidates.find(candidate => existsSync(candidate));
}

function validateRedirectTargets(routes: ParsedRoute[], label: EnvMode): string[] {
  const errors: string[] = [];
  const pathSet = new Set(routes.map(route => route.path));

  for (const route of routes) {
    if (route.componentKind === 'redirectTo' && route.redirectTarget) {
      const normalized = stripQueryAndHash(route.redirectTarget);
      if (!pathSet.has(normalized)) {
        errors.push(`[${label}] redirect target missing: ${route.path} -> ${route.redirectTarget}`);
      }
    }

    if (route.componentKind === 'redirectTelemetry' && !pathSet.has('/telemetry')) {
      errors.push(`[${label}] telemetry redirect missing base /telemetry for ${route.path}`);
    }
  }

  return errors;
}

function stripQueryAndHash(value: string): string {
  const hashIndex = value.indexOf('#');
  const queryIndex = value.indexOf('?');
  const cutoffCandidates = [hashIndex, queryIndex].filter(index => index >= 0);
  if (cutoffCandidates.length === 0) {
    return value;
  }
  const cutoff = Math.min(...cutoffCandidates);
  return value.slice(0, cutoff);
}

function validateRoleCoherence(routes: ParsedRoute[], label: EnvMode): string[] {
  const errors: string[] = [];

  for (const route of routes) {
    if (route.roleVisibility && route.roleVisibility.length === 0) {
      errors.push(`[${label}] ${route.path} has empty roleVisibility array`);
    }

    if (route.requiredRoles && route.roleVisibility) {
      const invalid = route.roleVisibility.filter(role => !route.requiredRoles!.includes(role));
      if (invalid.length > 0) {
        errors.push(
          `[${label}] ${route.path} roleVisibility includes roles without access (${invalid.join(', ')})`,
        );
      }
    }
  }

  return errors;
}

function validateModeFiltering(routes: ParsedRoute[], label: EnvMode): string[] {
  const errors: string[] = [];

  for (const route of routes) {
    if (!route.navTitle) {
      continue;
    }

    if (route.modes && route.modes.length === 0) {
      errors.push(`[${label}] ${route.path} has navTitle but empty modes`);
      continue;
    }

    if (route.modes) {
      const invalidModes = route.modes.filter(mode => !allowedModes.has(mode));
      if (invalidModes.length > 0) {
        errors.push(`[${label}] ${route.path} has invalid modes: ${invalidModes.join(', ')}`);
      }
    }
  }

  return errors;
}

function validateDevRoutesExcluded(prodRoutes: ParsedRoute[]): string[] {
  const errors: string[] = [];
  const devRoutes = prodRoutes.filter(route => route.path.startsWith('/dev') || route.path.startsWith('/_dev'));
  if (devRoutes.length > 0) {
    errors.push(`production bundle includes dev-only routes: ${devRoutes.map(r => r.path).join(', ')}`);
  }
  return errors;
}

function extractPrimarySpine(source: ts.SourceFile): string[] {
  let initializer: ts.Expression | undefined;

  source.forEachChild(node => {
    if (ts.isVariableStatement(node)) {
      node.declarationList.declarations.forEach(decl => {
        if (ts.isIdentifier(decl.name) && decl.name.text === 'PRIMARY_SPINE' && decl.initializer) {
          initializer = decl.initializer;
        }
      });
    }
  });

  const arrayLiteral = initializer ? toArrayLiteral(initializer) : undefined;

  if (!arrayLiteral) {
    throw new Error('Unable to locate PRIMARY_SPINE in routes_manifest.ts');
  }

  return arrayLiteral.elements
    .filter(ts.isStringLiteral)
    .map(element => element.text);
}

function toArrayLiteral(expr: ts.Expression): ts.ArrayLiteralExpression | undefined {
  if (ts.isArrayLiteralExpression(expr)) {
    return expr;
  }
  if (ts.isAsExpression(expr) || ts.isParenthesizedExpression(expr)) {
    return toArrayLiteral(expr.expression);
  }
  if (ts.isSatisfiesExpression(expr)) {
    return toArrayLiteral(expr.expression);
  }
  if (ts.isTypeAssertionExpression(expr)) {
    return toArrayLiteral(expr.expression);
  }
  return undefined;
}

function validatePrimarySpine(primarySpine: string[], prodRoutes: ParsedRoute[], devRoutes: ParsedRoute[]): string[] {
  const errors: string[] = [];
  const prodPaths = new Set(prodRoutes.map(route => route.path));
  const devPaths = new Set(devRoutes.map(route => route.path));

  for (const pathEntry of primarySpine) {
    const isDevOnly = pathEntry.startsWith('/dev') || pathEntry.startsWith('/_dev');
    if (isDevOnly) {
      if (!devPaths.has(pathEntry)) {
        errors.push(`[development] PRIMARY_SPINE includes ${pathEntry} but no matching dev route found`);
      }
      if (prodPaths.has(pathEntry)) {
        errors.push(`[production] dev route ${pathEntry} should not be present in prod routes`);
      }
    } else if (!prodPaths.has(pathEntry)) {
      errors.push(`[production] PRIMARY_SPINE entry ${pathEntry} missing from prod routes`);
    }
  }

  return errors;
}

/**
 * Option B Guardrail: Analyze component prop signatures using TypeScript compiler.
 * Detects components with required props that would crash when rendered without them.
 */
function validateComponentProps(lazyImports: Map<string, string>, routes: ParsedRoute[]): string[] {
  const errors: string[] = [];

  // Create a TypeScript program with the project's tsconfig
  const configPath = path.join(projectRoot, 'tsconfig.json');
  const configFile = ts.readConfigFile(configPath, ts.sys.readFile);
  if (configFile.error) {
    console.warn('Warning: Could not read tsconfig.json for prop analysis');
    return [];
  }

  const parsedConfig = ts.parseJsonConfigFileContent(configFile.config, ts.sys, projectRoot);

  // Collect component files to analyze
  const componentFiles: Array<{ name: string; importPath: string; filePath: string }> = [];

  for (const route of routes) {
    if (!route.componentName) continue;

    const importPath = lazyImports.get(route.componentName);
    if (!importPath) continue;

    const resolved = resolveImportPath(importPath);
    if (!resolved) continue;

    componentFiles.push({
      name: route.componentName,
      importPath,
      filePath: resolved,
    });
  }

  if (componentFiles.length === 0) return [];

  // Create program with all component files
  const program = ts.createProgram(
    componentFiles.map(c => c.filePath),
    parsedConfig.options
  );
  const checker = program.getTypeChecker();

  for (const { name, filePath } of componentFiles) {
    const sourceFile = program.getSourceFile(filePath);
    if (!sourceFile) continue;

    const requiredProps = getComponentRequiredProps(sourceFile, checker);
    if (requiredProps.length > 0) {
      errors.push(
        `Component "${name}" has required props: ${requiredProps.join(', ')}. ` +
        `Create a *RoutePage wrapper that reads params from URL and fetches data.`
      );
    }
  }

  return errors;
}

/**
 * Extract required props from a component's default export.
 * Returns array of required prop names, or empty array if no required props.
 */
function getComponentRequiredProps(sourceFile: ts.SourceFile, checker: ts.TypeChecker): string[] {
  const requiredProps: string[] = [];

  // Find the default export
  let defaultExportSymbol: ts.Symbol | undefined;

  for (const statement of sourceFile.statements) {
    // Handle: export default function Foo() {} or export default Foo
    if (ts.isExportAssignment(statement) && !statement.isExportEquals) {
      const expr = statement.expression;
      if (ts.isIdentifier(expr)) {
        defaultExportSymbol = checker.getSymbolAtLocation(expr);
      } else if (ts.isFunctionExpression(expr) || ts.isArrowFunction(expr)) {
        // Inline function - get its type directly
        const type = checker.getTypeAtLocation(expr);
        return extractRequiredPropsFromType(type, checker);
      }
    }
    // Handle: export default function Foo() {}
    if (ts.isFunctionDeclaration(statement) && statement.modifiers?.some(m => m.kind === ts.SyntaxKind.ExportKeyword && statement.modifiers?.some(m2 => m2.kind === ts.SyntaxKind.DefaultKeyword))) {
      const type = checker.getTypeAtLocation(statement);
      return extractRequiredPropsFromType(type, checker);
    }
  }

  if (defaultExportSymbol) {
    const type = checker.getTypeOfSymbolAtLocation(defaultExportSymbol, sourceFile);
    return extractRequiredPropsFromType(type, checker);
  }

  return requiredProps;
}

/**
 * Extract required props from a function/component type.
 */
function extractRequiredPropsFromType(type: ts.Type, checker: ts.TypeChecker): string[] {
  const requiredProps: string[] = [];

  // Get call signatures (for function components)
  const signatures = type.getCallSignatures();
  if (signatures.length === 0) return requiredProps;

  const firstSignature = signatures[0];
  const params = firstSignature.getParameters();

  if (params.length === 0) return requiredProps;

  // First parameter is props
  const propsParam = params[0];
  const propsType = checker.getTypeOfSymbolAtLocation(propsParam, propsParam.valueDeclaration!);

  // Check each property of the props type
  for (const prop of propsType.getProperties()) {
    const propDecl = prop.valueDeclaration;
    if (!propDecl) continue;

    // Check if the property is optional (has ? modifier)
    const isOptional = (prop.flags & ts.SymbolFlags.Optional) !== 0;

    if (!isOptional) {
      // Also check if property has undefined in its type (union with undefined)
      const propType = checker.getTypeOfSymbolAtLocation(prop, propDecl);
      const hasUndefined = propType.isUnion() && propType.types.some(t => t.flags & ts.TypeFlags.Undefined);

      if (!hasUndefined) {
        requiredProps.push(prop.name);
      }
    }
  }

  return requiredProps;
}

/**
 * Option C Guardrail: Detect type casts that bypass prop safety.
 * Patterns like `as any`, `as ComponentType<any>`, `as React.ComponentType<any>`
 * allow components with required props to slip through.
 */
function validateNoTypeCasts(sourceText: string): string[] {
  const errors: string[] = [];

  // Strip comments before checking for patterns (to avoid false positives from docs)
  const codeOnly = stripComments(sourceText);

  const dangerousPatterns = [
    { pattern: /as\s+any\b/g, description: '`as any` cast' },
    { pattern: /as\s+ComponentType<any>/g, description: '`as ComponentType<any>` cast' },
    { pattern: /as\s+React\.ComponentType<any>/g, description: '`as React.ComponentType<any>` cast' },
    { pattern: /as\s+LazyExoticComponent<any>/g, description: '`as LazyExoticComponent<any>` cast' },
    { pattern: /as\s+React\.LazyExoticComponent<any>/g, description: '`as React.LazyExoticComponent<any>` cast' },
  ];

  for (const { pattern, description } of dangerousPatterns) {
    const matches = codeOnly.match(pattern);
    if (matches) {
      errors.push(`routes.ts contains ${description} which bypasses prop type safety (${matches.length} occurrence(s))`);
    }
  }

  return errors;
}

/**
 * Strip single-line (//) and multi-line comments from source text.
 * Also strips string literals to avoid false positives.
 */
function stripComments(source: string): string {
  // Remove multi-line comments (/* ... */ and /** ... */)
  let result = source.replace(/\/\*[\s\S]*?\*\//g, '');
  // Remove single-line comments (// ...)
  result = result.replace(/\/\/.*$/gm, '');
  // Remove template literals and strings to avoid matching patterns inside them
  result = result.replace(/`[^`]*`/g, '""');
  result = result.replace(/'[^']*'/g, '""');
  result = result.replace(/"[^"]*"/g, '""');
  return result;
}

/**
 * Option C Guardrail: Detect modal/dialog components routed directly.
 * Components matching modal patterns (Modal, Dialog, Drawer, Sheet, Popup, Overlay)
 * likely have required props (open, onClose) and will crash when rendered without them.
 * Exception: *RoutePage wrappers are allowed as they handle the prop bridging.
 */
function validateNoModalComponents(routes: ParsedRoute[]): string[] {
  const errors: string[] = [];
  const modalPatterns = [
    /Modal$/,
    /Dialog$/,
    /Drawer$/,
    /Sheet$/,
    /Popup$/,
    /Overlay$/,
  ];

  for (const route of routes) {
    if (!route.componentName) {
      continue;
    }

    // *RoutePage wrappers are the correct pattern - don't flag them
    if (route.componentName.endsWith('RoutePage')) {
      continue;
    }

    for (const pattern of modalPatterns) {
      if (pattern.test(route.componentName)) {
        errors.push(
          `${route.path} uses component "${route.componentName}" which matches modal pattern ${pattern}. ` +
          `Modal components typically require props (open, onClose). Create a *RoutePage wrapper instead.`
        );
        break;
      }
    }
  }

  return errors;
}

main();
