import { createContext, useContext, useState, useEffect, useCallback, useRef, ReactNode } from 'react';
import { logger, toError } from '@/utils/logger';

export type BookmarkType = 'page' | 'adapter' | 'tenant' | 'policy' | 'node' | 'worker' | 'bundle' | 'event';

export interface Bookmark {
  id: string;
  type: BookmarkType;
  title: string;
  url: string;
  entityId?: string;
  description?: string;
  createdAt: string;
}

interface BookmarkContextValue {
  bookmarks: Bookmark[];
  addBookmark: (bookmark: Omit<Bookmark, 'id' | 'createdAt'>) => void;
  removeBookmark: (id: string) => void;
  isBookmarked: (url: string, entityId?: string) => boolean;
  getBookmark: (url: string, entityId?: string) => Bookmark | undefined;
  clearBookmarks: () => void;
}

const BookmarkContext = createContext<BookmarkContextValue | null>(null);

const STORAGE_KEY = 'aos_bookmarks';

export function BookmarkProvider({ children }: { children: ReactNode }) {
  const [bookmarks, setBookmarks] = useState<Bookmark[]>([]);
  const isUpdatingFromStorage = useRef(false);

  // Load bookmarks from localStorage on mount
  useEffect(() => {
    try {
      const saved = localStorage.getItem(STORAGE_KEY);
      if (saved) {
        const parsed = JSON.parse(saved);
        setBookmarks(Array.isArray(parsed) ? parsed : []);
      }
    } catch (err) {
      logger.error('Failed to load bookmarks', { component: 'BookmarkContext' }, toError(err));
    }
  }, []);

  // Save bookmarks to localStorage whenever they change
  // Note: Storage events only fire in OTHER tabs, not the tab that made the change
  useEffect(() => {
    // Skip saving if we're updating from a storage event (from another tab)
    if (isUpdatingFromStorage.current) {
      isUpdatingFromStorage.current = false;
      return;
    }

    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(bookmarks));
    } catch (err) {
      logger.error('Failed to save bookmarks', { component: 'BookmarkContext' }, toError(err));
    }
  }, [bookmarks]);

  // Listen for storage events from other tabs
  useEffect(() => {
    const handleStorageChange = (e: StorageEvent) => {
      if (e.key === STORAGE_KEY && e.newValue) {
        try {
          const parsed = JSON.parse(e.newValue);
          isUpdatingFromStorage.current = true;
          setBookmarks(Array.isArray(parsed) ? parsed : []);
        } catch (err) {
          logger.error('Failed to parse bookmarks from storage event', { component: 'BookmarkContext' }, toError(err));
        }
      }
    };

    window.addEventListener('storage', handleStorageChange);
    return () => window.removeEventListener('storage', handleStorageChange);
  }, []);

  const addBookmark = useCallback((bookmark: Omit<Bookmark, 'id' | 'createdAt'>) => {
    // Check if already bookmarked
    const existing = bookmarks.find(
      b => b.url === bookmark.url && (!bookmark.entityId || b.entityId === bookmark.entityId)
    );
    if (existing) {
      return; // Already bookmarked
    }

    const newBookmark: Bookmark = {
      ...bookmark,
      id: `bookmark-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`,
      createdAt: new Date().toISOString(),
    };

    setBookmarks(prev => [...prev, newBookmark]);
  }, [bookmarks]);

  const removeBookmark = useCallback((id: string) => {
    setBookmarks(prev => prev.filter(b => b.id !== id));
  }, []);

  const isBookmarked = useCallback((url: string, entityId?: string): boolean => {
    return bookmarks.some(
      b => b.url === url && (!entityId || b.entityId === entityId)
    );
  }, [bookmarks]);

  const getBookmark = useCallback((url: string, entityId?: string): Bookmark | undefined => {
    return bookmarks.find(
      b => b.url === url && (!entityId || b.entityId === entityId)
    );
  }, [bookmarks]);

  const clearBookmarks = useCallback(() => {
    setBookmarks([]);
  }, []);

  return (
    <BookmarkContext.Provider
      value={{
        bookmarks,
        addBookmark,
        removeBookmark,
        isBookmarked,
        getBookmark,
        clearBookmarks,
      }}
    >
      {children}
    </BookmarkContext.Provider>
  );
}

export function useBookmarks(): BookmarkContextValue {
  const context = useContext(BookmarkContext);
  if (!context) {
    throw new Error('useBookmarks must be used within BookmarkProvider');
  }
  return context;
}

