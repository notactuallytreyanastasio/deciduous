/**
 * useLocalStorage Hook
 *
 * Persists state to localStorage with automatic JSON serialization.
 * Falls back gracefully if localStorage is unavailable.
 */

import { useState, useEffect, useCallback } from 'react';

const STORAGE_PREFIX = 'deciduous_archaeology_';

export function useLocalStorage<T>(
  key: string,
  initialValue: T
): [T, (value: T | ((prev: T) => T)) => void, () => void] {
  const storageKey = STORAGE_PREFIX + key;

  // Initialize state from localStorage or use initial value
  const [storedValue, setStoredValue] = useState<T>(() => {
    if (typeof window === 'undefined') return initialValue;

    try {
      const item = window.localStorage.getItem(storageKey);
      return item ? (JSON.parse(item) as T) : initialValue;
    } catch (error) {
      console.warn(`Error reading localStorage key "${storageKey}":`, error);
      return initialValue;
    }
  });

  // Persist to localStorage when value changes
  useEffect(() => {
    if (typeof window === 'undefined') return;

    try {
      window.localStorage.setItem(storageKey, JSON.stringify(storedValue));
    } catch (error) {
      console.warn(`Error writing localStorage key "${storageKey}":`, error);
    }
  }, [storageKey, storedValue]);

  // Clear function
  const clear = useCallback(() => {
    try {
      window.localStorage.removeItem(storageKey);
      setStoredValue(initialValue);
    } catch (error) {
      console.warn(`Error clearing localStorage key "${storageKey}":`, error);
    }
  }, [storageKey, initialValue]);

  return [storedValue, setStoredValue, clear];
}

/**
 * Clear all archaeology-related localStorage entries
 */
export function clearAllArchaeologyStorage(): void {
  if (typeof window === 'undefined') return;

  const keysToRemove: string[] = [];
  for (let i = 0; i < window.localStorage.length; i++) {
    const key = window.localStorage.key(i);
    if (key?.startsWith(STORAGE_PREFIX)) {
      keysToRemove.push(key);
    }
  }

  keysToRemove.forEach(key => {
    try {
      window.localStorage.removeItem(key);
    } catch (error) {
      console.warn(`Error clearing localStorage key "${key}":`, error);
    }
  });
}
