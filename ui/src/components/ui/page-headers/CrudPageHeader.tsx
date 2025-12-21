import React from 'react';
import { BasePageHeader } from './BasePageHeader';
import { CrudPageHeaderProps } from './types';

// 【2025-01-20†rectification†crud_page_header_refactored】

export type { CrudPageHeaderProps } from './types';

export function CrudPageHeader(props: CrudPageHeaderProps) {
  return <BasePageHeader {...props} />;
}

