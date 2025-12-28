import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import { useLocation } from 'react-router-dom';
import type { UserRole } from '@/api/auth-types';
import { useAuth } from '@/providers/CoreProviders';

type CopilotMessageRole = 'user' | 'assistant' | 'system';

export interface CopilotMessage {
  id: string;
  role: CopilotMessageRole;
  content: string;
  hidden?: boolean;
  createdAt: number;
  metadata?: {
    adapterId?: string;
    adapterLabel?: string;
    systemContext?: string;
  };
}

interface CopilotScreenContext {
  heading: string | null;
  breadcrumbs: string[];
  testIds: string[];
  systemContext: string;
  adapterLabel: string;
}

interface CopilotContextValue {
  isOpen: boolean;
  assistantLabel: string;
  currentContext: {
    url: string;
    pageTitle: string;
    role: UserRole | null;
    screen: CopilotScreenContext;
  };
  messageHistory: CopilotMessage[];
  toggleDrawer: () => void;
  openDrawer: () => void;
  closeDrawer: () => void;
  addMessage: (role: CopilotMessageRole, content: string, options?: { hidden?: boolean }) => void;
  resetConversation: () => void;
}

const CopilotContext = createContext<CopilotContextValue | undefined>(undefined);

const generateId = () => {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `copilot-${Date.now()}-${Math.random().toString(16).slice(2)}`;
};

const getPageTitle = () => {
  if (typeof document === 'undefined') return 'Current page';
  const raw = document.title?.trim();
  if (!raw) return 'Current page';
  const primary = raw.split('|')[0]?.trim();
  return primary || raw;
};

const formatRole = (role: UserRole | null | undefined) => {
  if (!role) return 'member';
  return role.charAt(0).toUpperCase() + role.slice(1);
};

const normalizeText = (value?: string | null) => value?.replace(/\s+/g, ' ').trim() ?? '';

const isElementVisible = (element: Element) => {
  if (typeof window === 'undefined') return false;
  const htmlElement = element as HTMLElement;
  if (!htmlElement) return false;
  const hasLayout = htmlElement.offsetParent !== null || htmlElement.getClientRects().length > 0;
  if (!hasLayout) return false;
  const style = window.getComputedStyle(htmlElement);
  return style.visibility !== 'hidden' && style.display !== 'none' && style.opacity !== '0';
};

const readHeading = (): string | null => {
  if (typeof document === 'undefined') return null;
  const heading = document.querySelector('main h1') ?? document.querySelector('h1');
  const text = normalizeText(heading?.textContent);
  return text || null;
};

const readBreadcrumbs = (): string[] => {
  if (typeof document === 'undefined') return [];
  const selectors = ['[data-slot="breadcrumb-item"]', 'nav[aria-label="breadcrumb"] li', 'nav[aria-label="Breadcrumb"] li'];
  const seen = new Set<string>();
  const items: string[] = [];

  selectors.forEach((selector) => {
    const nodes = Array.from(document.querySelectorAll(selector));
    nodes.forEach((node) => {
      const text = normalizeText(node.textContent);
      if (!text || seen.has(text)) return;
      seen.add(text);
      items.push(text);
    });
  });

  return items;
};

const readActiveTestIds = (): string[] => {
  if (typeof document === 'undefined') return [];
  const seen = new Set<string>();
  const ids: string[] = [];

  document.querySelectorAll('[data-testid]').forEach((node) => {
    const htmlNode = node as HTMLElement;
    const id = normalizeText(htmlNode.dataset.testid ?? htmlNode.getAttribute('data-testid'));
    if (!id || seen.has(id)) return;
    if (!isElementVisible(htmlNode)) return;
    seen.add(id);
    ids.push(id);
  });

  return ids;
};

const buildScreenContext = (pageTitle: string): CopilotScreenContext => {
  const heading = readHeading();
  const breadcrumbs = readBreadcrumbs();
  const testIds = readActiveTestIds();
  const headingLabel = heading || pageTitle || 'Workspace Settings';
  const breadcrumbsSentence = breadcrumbs.length ? `Breadcrumbs: ${breadcrumbs.join(' > ')}.` : '';
  const testIdSentence = testIds.length ? `Active test IDs: ${testIds.join(', ')}.` : '';

  const systemContext = [`User is viewing ${headingLabel}.`, 'Error logs are visible.', breadcrumbsSentence, testIdSentence]
    .filter(Boolean)
    .join(' ');

  return {
    heading,
    breadcrumbs,
    testIds,
    systemContext,
    adapterLabel: `Viewing: ${heading || pageTitle || 'Dashboard'}`,
  };
};

export function CopilotProvider({ children }: { children: ReactNode }) {
  const { user } = useAuth();
  const location = useLocation();
  const [isOpen, setIsOpen] = useState(false);
  const [messageHistory, setMessageHistory] = useState<CopilotMessage[]>([]);
  const createContextSnapshot = useCallback((): CopilotContextValue['currentContext'] => {
    const pageTitle = getPageTitle();
    return {
      url: `${location.pathname}${location.search}`,
      pageTitle,
      role: user?.role ?? null,
      screen: buildScreenContext(pageTitle),
    };
  }, [location.pathname, location.search, user?.role]);

  const [currentContext, setCurrentContext] = useState<CopilotContextValue['currentContext']>(createContextSnapshot);

  const assistantLabel = useMemo(() => {
    if (user?.role === 'admin') return 'Admin Copilot';
    if (user?.role === 'viewer') return 'Assistant';
    return 'Copilot';
  }, [user?.role]);

  useEffect(() => {
    setCurrentContext(createContextSnapshot());
  }, [createContextSnapshot]);

  useEffect(() => {
    if (!isOpen) return;
    setCurrentContext(createContextSnapshot());
  }, [createContextSnapshot, isOpen]);

  const toggleDrawer = useCallback(() => setIsOpen((prev) => !prev), []);
  const openDrawer = useCallback(() => setIsOpen(true), []);
  const closeDrawer = useCallback(() => setIsOpen(false), []);

  const addMessage = useCallback(
    (role: CopilotMessageRole, content: string, options?: { hidden?: boolean }) => {
      setMessageHistory((prev) => [
        ...prev,
        {
          id: generateId(),
          role,
          content,
          hidden: options?.hidden,
          createdAt: Date.now(),
        },
      ]);
    },
    []
  );

  const resetConversation = useCallback(() => setMessageHistory([]), []);

  // Inject a hidden system prompt whenever the drawer opens with the latest context.
  useEffect(() => {
    if (!isOpen) return;
    const contextualPrompt = [
      currentContext.screen.systemContext,
      `User role: ${formatRole(currentContext.role)}.`,
      currentContext.screen.breadcrumbs.length ? `Breadcrumbs: ${currentContext.screen.breadcrumbs.join(' > ')}.` : null,
      currentContext.screen.testIds.length ? `Active test IDs: ${currentContext.screen.testIds.join(', ')}.` : null,
      `URL: ${currentContext.url}.`,
    ]
      .filter(Boolean)
      .join(' ');
    setMessageHistory((prev) => {
      const lastSystem = [...prev]
        .reverse()
        .find((msg) => msg.role === 'system' && msg.hidden && msg.metadata?.adapterId === 'current-page-context');
      if (lastSystem?.content === contextualPrompt) return prev;
      return [
        ...prev,
        {
          id: generateId(),
          role: 'system',
          content: contextualPrompt,
          hidden: true,
          createdAt: Date.now(),
          metadata: {
            adapterId: 'current-page-context',
            adapterLabel: currentContext.screen.adapterLabel,
            systemContext: currentContext.screen.systemContext,
          },
        },
      ];
    });
  }, [
    currentContext.role,
    currentContext.screen.adapterLabel,
    currentContext.screen.breadcrumbs,
    currentContext.screen.systemContext,
    currentContext.screen.testIds,
    currentContext.url,
    isOpen,
  ]);

  // Hotkey: Cmd/Ctrl + \ toggles the Copilot drawer globally when focus is not in an input.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const tag = target?.tagName?.toLowerCase();
      if (tag === 'input' || tag === 'textarea' || tag === 'select' || target?.isContentEditable) {
        return;
      }
      if (!(event.metaKey || event.ctrlKey)) return;
      if (event.key !== '\\') return;
      event.preventDefault();
      toggleDrawer();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [toggleDrawer]);

  const value: CopilotContextValue = useMemo(
    () => ({
      isOpen,
      assistantLabel,
      currentContext,
      messageHistory,
      toggleDrawer,
      openDrawer,
      closeDrawer,
      addMessage,
      resetConversation,
    }),
    [addMessage, assistantLabel, closeDrawer, currentContext, isOpen, messageHistory, toggleDrawer]
  );

  return <CopilotContext.Provider value={value}>{children}</CopilotContext.Provider>;
}

export function useCopilot() {
  const context = useContext(CopilotContext);
  if (!context) {
    throw new Error('useCopilot must be used within a CopilotProvider');
  }
  return context;
}
