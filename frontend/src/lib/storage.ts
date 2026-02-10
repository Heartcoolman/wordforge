/** Type-safe localStorage wrapper */

const PREFIX = 'eng_';

export const storage = {
  get<T>(key: string, fallback: T): T {
    try {
      const raw = localStorage.getItem(PREFIX + key);
      if (raw === null) return fallback;
      return JSON.parse(raw) as T;
    } catch {
      return fallback;
    }
  },

  set<T>(key: string, value: T): void {
    try {
      localStorage.setItem(PREFIX + key, JSON.stringify(value));
    } catch {
      // Storage full or unavailable
    }
  },

  remove(key: string): void {
    localStorage.removeItem(PREFIX + key);
  },

  getString(key: string, fallback = ''): string {
    try {
      return localStorage.getItem(PREFIX + key) ?? fallback;
    } catch {
      return fallback;
    }
  },

  setString(key: string, value: string): void {
    try {
      localStorage.setItem(PREFIX + key, value);
    } catch {
      // Storage full or unavailable
    }
  },
};

// Well-known storage keys
export const STORAGE_KEYS = {
  AUTH_TOKEN: 'auth_token',
  REFRESH_TOKEN: 'refresh_token',
  USER: 'user',
  THEME: 'theme',
  ADMIN_TOKEN: 'admin_token',
  LEARNING_MODE: 'learning_mode',
  LEARNING_QUEUE: 'learning_queue',
  LEARNING_SESSION_ID: 'learning_session_id',
} as const;
