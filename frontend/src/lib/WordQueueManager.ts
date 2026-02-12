import { createSignal } from 'solid-js';
import type { Word } from '@/types/word';
import { storage, STORAGE_KEYS } from './storage';
import { MASTERY_THRESHOLD, MAX_ANSWER_HISTORY, RECENT_WINDOW_SIZE } from './constants';

export interface QueuedWord {
  word: Word;
  correctCount: number;
  errorCount: number;
  lastShown: number;
  /** 后端排序优先级，0 = 最高优先级 */
  priority: number;
}

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
  let answerHistory: Array<{ wordId: string; correct: boolean; responseTimeMs: number }> = [];
  let _targetMasteryCount = 0;

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
      // 兼容旧数据：如果没有 priority 字段，按索引补充
      active = (saved.active ?? []).map((q, i) => ({
        ...q,
        priority: q.priority ?? i,
      }));
      mastered = (saved.mastered ?? []).map((q, i) => ({
        ...q,
        priority: q.priority ?? i,
      }));
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

    /** Internal: add words that are not already in queue,按传入顺序设置 priority */
    _mergeNewWords(words: Word[]) {
      const existingIds = new Set([
        ...active.map((q) => q.word.id),
        ...mastered.map((q) => q.word.id),
      ]);
      // 新词的 priority 从当前最大值 +1 开始，保证追加的词排在已有词之后
      let nextPriority = active.length > 0
        ? Math.max(...active.map((q) => q.priority)) + 1
        : 0;
      for (const w of words) {
        if (!existingIds.has(w.id)) {
          active.push({ word: w, correctCount: 0, errorCount: 0, lastShown: 0, priority: nextPriority++ });
        }
      }
      persist();
      syncSignals();
    },

    /** Load initial words from backend study-words */
    loadWords(words: Word[]) {
      this._mergeNewWords(words);
    },

    /** Add more words from next-words */
    addWords(words: Word[]) {
      this._mergeNewWords(words);
    },

    /** 选词逻辑：错误词优先，否则按后端排序（priority 升序），同 priority 按 lastShown 升序 */
    pickNext(): QueuedWord | null {
      if (active.length === 0) return null;

      // 优先选有错误的词（按 lastShown 升序，最久未展示的先复习）
      const withErrors = active.filter((q) => q.errorCount > 0);
      if (withErrors.length > 0) {
        withErrors.sort((a, b) => a.lastShown - b.lastShown);
        return withErrors[0];
      }

      // 无错误词时，按 priority 升序（尊重后端 AMAS 排序），同 priority 按 lastShown 升序
      const sorted = [...active].sort((a, b) => {
        if (a.priority !== b.priority) return a.priority - b.priority;
        return a.lastShown - b.lastShown;
      });
      return sorted[0];
    },

    /** Record answer result */
    recordAnswer(wordId: string, correct: boolean, responseTimeMs?: number): { mastered: boolean } {
      const item = active.find((q) => q.word.id === wordId);
      if (!item) return { mastered: false };

      if (responseTimeMs != null) {
        answerHistory.push({ wordId, correct, responseTimeMs });
        // 限制历史记录最大长度，防止内存无限增长
        if (answerHistory.length > MAX_ANSWER_HISTORY) {
          answerHistory = answerHistory.slice(-MAX_ANSWER_HISTORY);
        }
      }

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

    /** 设置本次会话的掌握目标 */
    setTargetMasteryCount(count: number) {
      _targetMasteryCount = count;
    },

    /** 计算会话指标 */
    computeSessionMetrics(): { recentAccuracy: number; overallAccuracy: number; recentAvgResponseTimeMs: number; overallAvgResponseTimeMs: number } {
      if (answerHistory.length === 0) {
        return { recentAccuracy: 0, overallAccuracy: 0, recentAvgResponseTimeMs: 0, overallAvgResponseTimeMs: 0 };
      }

      const overall = answerHistory.reduce(
        (acc, h) => ({ correct: acc.correct + (h.correct ? 1 : 0), time: acc.time + h.responseTimeMs }),
        { correct: 0, time: 0 },
      );
      const overallAccuracy = overall.correct / answerHistory.length;
      const overallAvgResponseTimeMs = overall.time / answerHistory.length;

      // 最近 5 题
      const recent = answerHistory.slice(-RECENT_WINDOW_SIZE);
      const recentCorrect = recent.filter((h) => h.correct).length;
      const recentAccuracy = recentCorrect / recent.length;
      const recentAvgResponseTimeMs = recent.reduce((sum, h) => sum + h.responseTimeMs, 0) / recent.length;

      return { recentAccuracy, overallAccuracy, recentAvgResponseTimeMs, overallAvgResponseTimeMs };
    },

    /** 是否应该预取更多词 - active 队列剩余 <= 2 且未达目标 */
    shouldPrefetch(): boolean {
      return active.length <= 2 && (_targetMasteryCount === 0 || mastered.length < _targetMasteryCount);
    },

    /** 获取 error-prone 词 ID（错误次数 > 0 的词）*/
    getErrorProneWordIds(): string[] {
      return [
        ...active.filter((q) => q.errorCount > 0).map((q) => q.word.id),
        ...mastered.filter((q) => q.errorCount > 0).map((q) => q.word.id),
      ];
    },

    /** 重置答题历史（会话开始时调用）*/
    resetHistory() {
      answerHistory = [];
    },

    /** Reset for new session */
    reset() {
      active = [];
      mastered = [];
      answerHistory = [];
      _targetMasteryCount = 0;
      storage.remove(STORAGE_KEYS.LEARNING_QUEUE);
      syncSignals();
    },
  };
}

/** Type of the queue manager returned by createWordQueueManager */
export type WordQueueManager = ReturnType<typeof createWordQueueManager>;
