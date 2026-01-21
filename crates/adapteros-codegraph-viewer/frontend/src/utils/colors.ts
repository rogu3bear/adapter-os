// Color scheme for different symbol kinds and states

export const NODE_COLORS = {
  function: '#3b82f6', // blue
  struct: '#10b981', // green
  trait: '#fbbf24', // yellow
  impl: '#f87171', // coral
  module: '#9ca3af', // gray
  enum: '#8b5cf6', // purple
  type: '#06b6d4', // cyan
  const: '#ec4899', // pink
  static: '#f97316', // orange
  macro: '#a855f7', // violet
  field: '#14b8a6', // teal
  variant: '#84cc16', // lime
  method: '#6366f1', // indigo
  associated_type: '#d946ef', // fuchsia
  associated_const: '#f43f5e', // rose
  default: '#6b7280', // gray-500
};

export const EDGE_COLORS = {
  normal: '#9ca3af', // gray
  recursive: '#ef4444', // red
  trait_call: '#fbbf24', // yellow
  generic: '#3b82f6', // blue
};

export const DIFF_COLORS = {
  added: '#10b981', // green
  removed: '#ef4444', // red
  modified: '#f59e0b', // amber/orange
  unchanged: '#6b7280', // gray
};

export const getNodeColor = (kind: string): string => {
  const normalized = kind.toLowerCase();
  return NODE_COLORS[normalized as keyof typeof NODE_COLORS] || NODE_COLORS.default;
};

export const getEdgeColor = (
  isRecursive: boolean,
  isTraitCall: boolean,
  isGeneric: boolean
): string => {
  if (isRecursive) return EDGE_COLORS.recursive;
  if (isTraitCall) return EDGE_COLORS.trait_call;
  if (isGeneric) return EDGE_COLORS.generic;
  return EDGE_COLORS.normal;
};

export const getDiffColor = (state: 'added' | 'removed' | 'modified' | 'unchanged'): string => {
  return DIFF_COLORS[state];
};

