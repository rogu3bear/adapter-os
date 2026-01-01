import type { Config } from 'tailwindcss';

export default {
  theme: {
    extend: {
      fontSize: {
        'xxs': ['10px', { lineHeight: '1' }],
        'xxxs': ['9px', { lineHeight: '1' }],
      },
    },
  },
} satisfies Config;
