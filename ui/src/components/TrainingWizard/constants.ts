import { Code, Zap, GitBranch, Clock } from 'lucide-react';

export const CATEGORY_ICONS = {
  code: Code,
  framework: Zap,
  codebase: GitBranch,
  ephemeral: Clock,
};

export const CATEGORY_DESCRIPTIONS = {
  code: 'Language-specific adapters for syntax, idioms, and patterns',
  framework: 'Framework-specific adapters for APIs and best practices',
  codebase: 'Repository-specific adapters trained on your codebase',
  ephemeral: 'Short-lived adapters for specific tasks or contexts',
};

export const LANGUAGES = [
  'TypeScript', 'JavaScript', 'Python', 'Rust', 'Go', 'Java', 'C++', 'C#', 'Ruby', 'PHP',
];

export const LORA_TARGETS = [
  'q_proj', 'k_proj', 'v_proj', 'o_proj',
  'gate_proj', 'up_proj', 'down_proj',
  'embed_tokens', 'lm_head',
];

export const FILE_VALIDATION = {
  maxSize: 100 * 1024 * 1024,
  allowedExtensions: ['.pdf', '.txt', '.json', '.jsonl', '.csv', '.md', '.py', '.js', '.ts', '.tsx', '.jsx', '.rs', '.go', '.java'],
};
