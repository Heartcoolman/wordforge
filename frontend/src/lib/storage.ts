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
  FATIGUE_ENABLED: 'fatigue_enabled',
} as const;

/** Type-safe storage wrapper */

const PREFIX = 'eng_';
const SESSION_BACKED_KEYS = new Set<string>([]);

function isSessionBackedKey(key: string): boolean {
  return SESSION_BACKED_KEYS.has(key);
}

function getPrimaryStorage(key: string): Storage {
  return isSessionBackedKey(key) ? sessionStorage : localStorage;
}

function getLegacyStorage(key: string): Storage | null {
  return isSessionBackedKey(key) ? localStorage : null;
}

function readRaw(key: string): string | null {
  try {
    const namespacedKey = PREFIX + key;
    const primary = getPrimaryStorage(key);
    const value = primary.getItem(namespacedKey);
    if (value !== null) {
      return value;
    }

    const legacy = getLegacyStorage(key);
    if (!legacy) {
      return null;
    }

    const legacyValue = legacy.getItem(namespacedKey);
    if (legacyValue === null) {
      return null;
    }

    try {
      primary.setItem(namespacedKey, legacyValue);
    } catch {
      // sessionStorage unavailable, keep legacy value for this read
    }
    legacy.removeItem(namespacedKey);
    return legacyValue;
  } catch {
    return null;
  }
}

function writeRaw(key: string, value: string): void {
  try {
    const namespacedKey = PREFIX + key;
    getPrimaryStorage(key).setItem(namespacedKey, value);
    if (isSessionBackedKey(key)) {
      localStorage.removeItem(namespacedKey);
    }
  } catch {
    // Storage full or unavailable
  }
}

export const storage = {
  get<T>(key: string, fallback: T): T {
    try {
      const raw = readRaw(key);
      if (raw === null) return fallback;
      const parsed = JSON.parse(raw);
      // 基本类型检查：确保解析结果与 fallback 类型一致
      if (fallback !== null && fallback !== undefined && typeof parsed !== typeof fallback) {
        return fallback;
      }
      return parsed as T;
    } catch {
      return fallback;
    }
  },

  set<T>(key: string, value: T): void {
    writeRaw(key, JSON.stringify(value));
  },

  remove(key: string): void {
    try {
      const namespacedKey = PREFIX + key;
      getPrimaryStorage(key).removeItem(namespacedKey);
      if (isSessionBackedKey(key)) {
        localStorage.removeItem(namespacedKey);
      }
    } catch {
      // Storage unavailable
    }
  },

  getString(key: string, fallback = ''): string {
    const value = readRaw(key);
    return value ?? fallback;
  },

  setString(key: string, value: string): void {
    writeRaw(key, value);
  },
};
