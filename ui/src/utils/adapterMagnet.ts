import type { AttachedAdapter, SuggestedAdapter } from '@/contexts/ChatContext';

export type MagnetAura = 'code' | 'data' | 'creative' | 'ops' | 'safety' | 'general';

export interface MagnetClassification {
  aura: MagnetAura;
  color: string;
  label: string;
}

export interface ConflictCheck {
  aura: MagnetAura;
  decision: 'attach' | 'replace' | 'skip';
  conflicts: AttachedAdapter[];
  reason?: string;
}

const AURA_COLORS: Record<MagnetAura, string> = {
  code: '#2563eb', // blue
  data: '#0ea5e9', // cyan/analytics
  creative: '#22c55e', // green
  ops: '#a855f7', // purple
  safety: '#f97316', // amber
  general: '#6b7280', // slate
};

const EXCLUSIVE_AURAS: Set<MagnetAura> = new Set(['safety', 'ops']);

const HARD_CONFLICT_KEYWORDS = ['guard', 'moderation', 'policy', 'shield'];

function normalizeText(parts: Array<string | undefined>): string {
  return parts
    .filter(Boolean)
    .join(' ')
    .toLowerCase();
}

export function classifyMagnet(adapter: SuggestedAdapter | AttachedAdapter | null): MagnetClassification {
  if (!adapter) {
    return { aura: 'general', color: AURA_COLORS.general, label: 'General' };
  }

  const blob = normalizeText([adapter.id, adapter.reason, adapter.keywords?.join(' ')]);

  if (/(sql|data|warehouse|analytics|bi|bigquery|snowflake|duckdb|pandas)/.test(blob)) {
    return { aura: 'data', color: AURA_COLORS.data, label: 'Data' };
  }
  if (/(code|python|typescript|javascript|rust|go|java|api|backend)/.test(blob)) {
    return { aura: 'code', color: AURA_COLORS.code, label: 'Code' };
  }
  if (/(creative|story|write|design|image|vision|brand|marketing|canvas)/.test(blob)) {
    return { aura: 'creative', color: AURA_COLORS.creative, label: 'Creative' };
  }
  if (/(ops|orchestrat|deploy|pipeline|admin|maintain|platform)/.test(blob)) {
    return { aura: 'ops', color: AURA_COLORS.ops, label: 'Ops' };
  }
  if (/(safety|guard|moderation|compliance|policy|risk|shield)/.test(blob)) {
    return { aura: 'safety', color: AURA_COLORS.safety, label: 'Safety' };
  }

  return { aura: 'general', color: AURA_COLORS.general, label: 'General' };
}

export function resolveAdapterConflicts(
  candidate: SuggestedAdapter,
  attachedAdapters: AttachedAdapter[]
): ConflictCheck {
  const candidateClass = classifyMagnet(candidate);
  const alreadyAttached = attachedAdapters.some((adapter) => adapter.id === candidate.id);

  if (alreadyAttached) {
    return {
      aura: candidateClass.aura,
      conflicts: [],
      decision: 'skip',
      reason: 'Already attached',
    };
  }

  const conflicts = attachedAdapters.filter((adapter) => {
    const attachedClass = classifyMagnet(adapter);
    if (EXCLUSIVE_AURAS.has(attachedClass.aura) && attachedClass.aura === candidateClass.aura) {
      return true;
    }
    const hasHardConflict =
      HARD_CONFLICT_KEYWORDS.some((kw) => candidate.id.toLowerCase().includes(kw)) &&
      HARD_CONFLICT_KEYWORDS.some((kw) => adapter.id.toLowerCase().includes(kw));

    return hasHardConflict;
  });

  if (conflicts.length > 0) {
    return {
      aura: candidateClass.aura,
      conflicts,
      decision: 'replace',
      reason: 'Conflicts with an exclusive adapter already attached',
    };
  }

  return {
    aura: candidateClass.aura,
    conflicts: [],
    decision: 'attach',
  };
}

export function colorWithAlpha(hexColor: string, alpha: number): string {
  const normalized = hexColor.replace('#', '');
  const int = parseInt(normalized, 16);
  const r = (int >> 16) & 255;
  const g = (int >> 8) & 255;
  const b = int & 255;
  const clampedAlpha = Math.min(1, Math.max(0, alpha));
  return `rgba(${r}, ${g}, ${b}, ${clampedAlpha})`;
}

let cachedAudioContext: AudioContext | null = null;

export function playMagnetSnapFeedback(): void {
  if (typeof window === 'undefined') return;

  // Light haptic if supported
  if (navigator.vibrate) {
    try {
      navigator.vibrate(8);
    } catch {
      // ignore
    }
  }

  // Subtle click using Web Audio (no external asset)
  try {
    if (!cachedAudioContext) {
      cachedAudioContext = new (window.AudioContext || (window as any).webkitAudioContext)();
    }
    const ctx = cachedAudioContext;
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();

    osc.frequency.value = 220;
    gain.gain.value = 0.0001;

    osc.connect(gain);
    gain.connect(ctx.destination);

    const now = ctx.currentTime;
    gain.gain.exponentialRampToValueAtTime(0.04, now + 0.01);
    gain.gain.exponentialRampToValueAtTime(0.00001, now + 0.12);

    osc.start(now);
    osc.stop(now + 0.15);
  } catch {
    // Silently ignore audio failures
  }
}
