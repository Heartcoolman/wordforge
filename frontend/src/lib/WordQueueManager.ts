import { createSignal } from 'solid-js';
import type { Word } from '@/types/word';
import { storage, STORAGE_KEYS } from './storage';

export interface QueuedWord {
  word: Word;
  correctCount: number;
  errorCount: number;
  lastShown: number;
}

const MASTERY_THRESHOLD = 2; // consecutive correct to consider mastered

/** Fisher-Yates shuffle (unbiased) */
function shuffle<T>(arr: T[]): T[] {
  const a = [...arr];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

/**
 * Create a reactive WordQueueManager.
 * Returns an object with SolidJS-reactive `activeCount` and `masteredCount` signals.
 */
export function createWordQueueManager(batchSize = 5) {
  let active: QueuedWord[] = [];
  let mastered: QueuedWord[] = [];
  let _batchSize = batchSize;

  // Reactive signals for UI binding
  const [activeCount, setActiveCount] = createSignal(0);
  const [masteredCount, setMasteredCount] = createSignal(0);

  function syncSignals() {
    setActiveCount(active.length);
    setMasteredCount(mastered.length);
  }

  function persist() {
    storage.set(STORAGE_KEYS.LEARNING_QUEUE, {
      active,
      mastered,
      batchSize: _batchSize,
    });
  }

  function restore() {
    const saved = storage.get<{
      active: QueuedWord[];
      mastered: QueuedWord[];
      batchSize: number;
    } | null>(STORAGE_KEYS.LEARNING_QUEUE, null);
    if (saved) {
      active = saved.active ?? [];
      mastered = saved.mastered ?? [];
      _batchSize = saved.batchSize ?? _batchSize;
    }
    syncSignals();
  }

  // Restore on creation
  restore();

  return {
    /** Reactive signal: number of active words */
    activeCount,
    /** Reactive signal: number of mastered words */
    masteredCount,

    /** Load initial words from backend study-words */
    loadWords(words: Word[]) {
      const existingIds = new Set([
        ...active.map((q) => q.word.id),
        ...mastered.map((q) => q.word.id),
      ]);
      for (const w of words) {
        if (!existingIds.has(w.id)) {
          active.push({ word: w, correctCount: 0, errorCount: 0, lastShown: 0 });
        }
      }
      persist();
      syncSignals();
    },

    /** Add more words from next-words */
    addWords(words: Word[]) {
      const existingIds = new Set([
        ...active.map((q) => q.word.id),
        ...mastered.map((q) => q.word.id),
      ]);
      for (const w of words) {
        if (!existingIds.has(w.id)) {
          active.push({ word: w, correctCount: 0, errorCount: 0, lastShown: 0 });
        }
      }
      persist();
      syncSignals();
    },

    /** Pick next word to show: prioritize errors, then least recently shown */
    pickNext(): QueuedWord | null {
      if (active.length === 0) return null;

      // Prioritize words with errors
      const withErrors = active.filter((q) => q.errorCount > 0);
      if (withErrors.length > 0) {
        withErrors.sort((a, b) => a.lastShown - b.lastShown);
        return withErrors[0];
      }

      // Otherwise, least recently shown
      const sorted = [...active].sort((a, b) => a.lastShown - b.lastShown);
      return sorted[0];
    },

    /** Record answer result */
    recordAnswer(wordId: string, correct: boolean): { mastered: boolean } {
      const item = active.find((q) => q.word.id === wordId);
      if (!item) return { mastered: false };

      item.lastShown = Date.now();
      if (correct) {
        item.correctCount++;
        item.errorCount = 0;

        if (item.correctCount >= MASTERY_THRESHOLD) {
          active = active.filter((q) => q.word.id !== wordId);
          mastered.push(item);
          persist();
          syncSignals();
          return { mastered: true };
        }
      } else {
        item.correctCount = 0;
        item.errorCount++;
      }

      persist();
      syncSignals();
      return { mastered: false };
    },

    /** Generate 4 options (1 correct + 3 distractors) from current pool */
    generateOptions(targetWord: QueuedWord, mode: 'word-to-meaning' | 'meaning-to-word'): string[] {
      const allWords = [...active, ...mastered];
      const correctAnswer = mode === 'word-to-meaning' ? targetWord.word.meaning : targetWord.word.text;

      const others = shuffle(
        allWords.filter((q) => q.word.id !== targetWord.word.id),
      )
        .slice(0, 3)
        .map((q) => (mode === 'word-to-meaning' ? q.word.meaning : q.word.text));

      while (others.length < 3) {
        others.push(mode === 'word-to-meaning' ? '(无释义)' : '(unknown)');
      }

      return shuffle([correctAnswer, ...others]);
    },

    /** Check if we need more words */
    needsMoreWords(): boolean {
      return active.length < _batchSize;
    },

    /** Get IDs of all words currently in queue */
    getAllWordIds(): string[] {
      return [...active.map((q) => q.word.id), ...mastered.map((q) => q.word.id)];
    },

    getMasteredWordIds(): string[] {
      return mastered.map((q) => q.word.id);
    },

    getActiveCount(): number { return active.length; },
    getMasteredCount(): number { return mastered.length; },

    setBatchSize(size: number) { _batchSize = size; },

    /** Reset for new session */
    reset() {
      active = [];
      mastered = [];
      storage.remove(STORAGE_KEYS.LEARNING_QUEUE);
      syncSignals();
    },
  };
}

/** Type of the queue manager returned by createWordQueueManager */
export type WordQueueManager = ReturnType<typeof createWordQueueManager>;
