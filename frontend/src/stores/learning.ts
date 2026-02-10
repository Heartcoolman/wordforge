import { createSignal, createRoot } from 'solid-js';
import { storage, STORAGE_KEYS } from '@/lib/storage';

export type LearningMode = 'word-to-meaning' | 'meaning-to-word';

function createLearningStore() {
  const savedMode = storage.get<LearningMode>(STORAGE_KEYS.LEARNING_MODE, 'word-to-meaning');
  const [mode, setMode] = createSignal<LearningMode>(savedMode);
  const [sessionId, setSessionId] = createSignal<string | null>(
    storage.getString(STORAGE_KEYS.LEARNING_SESSION_ID) || null,
  );

  function toggleMode() {
    const next = mode() === 'word-to-meaning' ? 'meaning-to-word' : 'word-to-meaning';
    setMode(next);
    storage.set(STORAGE_KEYS.LEARNING_MODE, next);
  }

  function setLearningMode(m: LearningMode) {
    setMode(m);
    storage.set(STORAGE_KEYS.LEARNING_MODE, m);
  }

  function startSession(id: string) {
    setSessionId(id);
    storage.setString(STORAGE_KEYS.LEARNING_SESSION_ID, id);
  }

  function clearSession() {
    setSessionId(null);
    storage.remove(STORAGE_KEYS.LEARNING_SESSION_ID);
    storage.remove(STORAGE_KEYS.LEARNING_QUEUE);
  }

  return {
    mode,
    setMode: setLearningMode,
    toggleMode,
    sessionId,
    startSession,
    clearSession,
  };
}

export const learningStore = createRoot(createLearningStore);
