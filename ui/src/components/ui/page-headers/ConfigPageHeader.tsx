import React from 'react';
import { BasePageHeader } from './BasePageHeader';
import { ConfigPageHeaderProps } from './types';

// 【2025-01-20†rectification†config_page_header_refactored】

export { ConfigPageHeaderProps } from './types';

export function ConfigPageHeader(props: ConfigPageHeaderProps) {
  return <BasePageHeader {...props} />;
}

