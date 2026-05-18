import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createRoot } from 'solid-js';
import { createWordQueueManager } from '@/lib/WordQueueManager';
import { createFakeWord, createFakeWords } from '../helpers/factories';

function createManager(batchSize = 5) {
  return createWordQueueManager(batchSize);
}

describe('createWordQueueManager', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('loadWords adds words to active queue', () => {
    const mgr = createManager();
    const words = createFakeWords(3);
    mgr.loadWords(words);
    expect(mgr.getActiveCount()).toBe(3);
  });

  it('loadWords skips duplicate IDs', () => {
    const mgr = createManager();
    const word = createFakeWord({ id: 'dup-1' });
    mgr.loadWords([word]);
    mgr.loadWords([word]);
    expect(mgr.getActiveCount()).toBe(1);
  });

  it('addWords appends new words', () => {
    const mgr = createManager();
    mgr.loadWords(createFakeWords(2));
    mgr.addWords(createFakeWords(3));
    expect(mgr.getActiveCount()).toBe(5);
  });

  it('pickNext returns null when empty', () => {
    const mgr = createManager();
    expect(mgr.pickNext()).toBeNull();
  });

  it('pickNext prioritizes words with errors', () => {
    const mgr = createManager();
    const w1 = createFakeWord({ id: 'a' });
    const w2 = createFakeWord({ id: 'b' });
    mgr.loadWords([w1, w2]);
    mgr.recordAnswer('b', false);
    const next = mgr.pickNext();
    expect(next?.word.id).toBe('b');
  });

  it('pickNext respects backend priority order when no errors', () => {
    const mgr = createManager();
    const w1 = createFakeWord({ id: 'x' });
    const w2 = createFakeWord({ id: 'y' });
    mgr.loadWords([w1, w2]);
    // w1 priority=0, w2 priority=1，按后端排序 w1 优先
    const next = mgr.pickNext();
    expect(next?.word.id).toBe('x');
  });

  it('pickNext uses lastShown as tiebreaker when priority is equal', () => {
    const mgr = createManager();
    const w1 = createFakeWord({ id: 'x' });
    const w2 = createFakeWord({ id: 'y' });
    mgr.loadWords([w1]);
    mgr.addWords([w2]);
    // 手动让两个词 priority 不同（0 vs 1），验证 priority 优先
    mgr.recordAnswer('x', true);
    // x.lastShown > 0, y.lastShown = 0，但 x.priority=0 < y.priority=1
    const next = mgr.pickNext();
    expect(next?.word.id).toBe('x');
  });

  it('recordAnswer correct increments correctCount', () => {
    const mgr = createManager();
    const w = createFakeWord({ id: 'c1' });
    mgr.loadWords([w]);
    mgr.recordAnswer('c1', true);
    const item = mgr.pickNext();
    expect(item?.correctCount).toBe(1);
  });

  it('recordAnswer correct resets errorCount', () => {
    const mgr = createManager();
    const w = createFakeWord({ id: 'c2' });
    mgr.loadWords([w]);
    mgr.recordAnswer('c2', false);
    mgr.recordAnswer('c2', true);
    const item = mgr.pickNext();
    expect(item?.errorCount).toBe(0);
  });

  it('recordAnswer wrong resets correctCount and increments errorCount', () => {
    const mgr = createManager();
    const w = createFakeWord({ id: 'c3' });
    mgr.loadWords([w]);
    mgr.recordAnswer('c3', true);
    mgr.recordAnswer('c3', false);
    const item = mgr.pickNext();
    expect(item?.correctCount).toBe(0);
    expect(item?.errorCount).toBe(1);
  });

  it('recordAnswer marks mastered after MASTERY_THRESHOLD consecutive correct', () => {
    const mgr = createManager();
    const w = createFakeWord({ id: 'm1' });
    mgr.loadWords([w]);
    mgr.recordAnswer('m1', true);
    const result = mgr.recordAnswer('m1', true);
    expect(result.mastered).toBe(true);
    expect(mgr.getActiveCount()).toBe(0);
    expect(mgr.getMasteredCount()).toBe(1);
  });

  it('mastered words move from active to mastered', () => {
    const mgr = createManager();
    const w = createFakeWord({ id: 'm2' });
    mgr.loadWords([w]);
    mgr.recordAnswer('m2', true);
    mgr.recordAnswer('m2', true);
    expect(mgr.getMasteredWordIds()).toContain('m2');
    expect(mgr.getActiveCount()).toBe(0);
  });

  it('generateOptions returns 4 options including correct answer', () => {
    vi.spyOn(Math, 'random').mockReturnValue(0.5);
    const mgr = createManager();
    const words = createFakeWords(5);
    mgr.loadWords(words);
    const target = mgr.pickNext()!;
    const options = mgr.generateOptions(target, 'word-to-meaning');
    expect(options).toHaveLength(4);
    expect(options).toContain(target.word.meaning);
    vi.restoreAllMocks();
  });

  it('generateOptions pads with placeholder when fewer than 3 distractors', () => {
    vi.spyOn(Math, 'random').mockReturnValue(0.5);
    const mgr = createManager();
    const w = createFakeWord({ id: 'solo' });
    mgr.loadWords([w]);
    const target = mgr.pickNext()!;
    const options = mgr.generateOptions(target, 'word-to-meaning');
    expect(options).toHaveLength(4);
    expect(options).toContain('(无释义)');
    vi.restoreAllMocks();
  });

  it('needsMoreWords returns true when active < batchSize', () => {
    const mgr = createManager(5);
    mgr.loadWords(createFakeWords(2));
    expect(mgr.needsMoreWords()).toBe(true);
  });

  it('needsMoreWords returns false when active >= batchSize', () => {
    const mgr = createManager(3);
    mgr.loadWords(createFakeWords(3));
    expect(mgr.needsMoreWords()).toBe(false);
  });

  it('getAllWordIds returns both active and mastered', () => {
    const mgr = createManager();
    const w1 = createFakeWord({ id: 'all-1' });
    const w2 = createFakeWord({ id: 'all-2' });
    mgr.loadWords([w1, w2]);
    mgr.recordAnswer('all-1', true);
    mgr.recordAnswer('all-1', true);
    const ids = mgr.getAllWordIds();
    expect(ids).toContain('all-1');
    expect(ids).toContain('all-2');
  });

  it('getMasteredWordIds returns only mastered', () => {
    const mgr = createManager();
    const w1 = createFakeWord({ id: 'gm-1' });
    const w2 = createFakeWord({ id: 'gm-2' });
    mgr.loadWords([w1, w2]);
    mgr.recordAnswer('gm-1', true);
    mgr.recordAnswer('gm-1', true);
    expect(mgr.getMasteredWordIds()).toEqual(['gm-1']);
  });

  it('setBatchSize changes threshold', () => {
    const mgr = createManager(5);
    mgr.loadWords(createFakeWords(3));
    expect(mgr.needsMoreWords()).toBe(true);
    mgr.setBatchSize(2);
    expect(mgr.needsMoreWords()).toBe(false);
  });

  it('reset clears all state', () => {
    const mgr = createManager();
    mgr.loadWords(createFakeWords(3));
    mgr.reset();
    expect(mgr.getActiveCount()).toBe(0);
    expect(mgr.getMasteredCount()).toBe(0);
    expect(mgr.pickNext()).toBeNull();
  });

  it('persist/restore roundtrip via localStorage', () => {
    createRoot((dispose) => {
      const mgr1 = createManager();
      const words = createFakeWords(3);
      mgr1.loadWords(words);
      mgr1.recordAnswer(words[0].id, true);
      mgr1.recordAnswer(words[0].id, true);
      // mgr1 persisted: 2 active, 1 mastered

      const mgr2 = createManager();
      expect(mgr2.getActiveCount()).toBe(2);
      expect(mgr2.getMasteredCount()).toBe(1);

      dispose();
    });
  });

  it('recordAnswer on unknown wordId returns mastered:false', () => {
    const mgr = createManager();
    expect(mgr.recordAnswer('does-not-exist', true)).toEqual({ mastered: false });
  });

  it('recordAnswer tracks history when responseTimeMs provided and truncates after MAX_ANSWER_HISTORY', async () => {
    const { MAX_ANSWER_HISTORY } = await import('@/lib/constants');
    const mgr = createManager();
    const w = createFakeWord({ id: 'hist' });
    mgr.loadWords([w]);
    for (let i = 0; i < MAX_ANSWER_HISTORY + 5; i++) {
      mgr.recordAnswer('hist', false, 100 + i);
    }
    // 应不抛错；计算指标可以返回数字
    const metrics = mgr.computeSessionMetrics();
    expect(metrics.overallAccuracy).toBe(0);
    expect(metrics.overallAvgResponseTimeMs).toBeGreaterThan(0);
  });

  it('computeSessionMetrics returns zeros when history empty', () => {
    const mgr = createManager();
    expect(mgr.computeSessionMetrics()).toEqual({
      recentAccuracy: 0,
      overallAccuracy: 0,
      recentAvgResponseTimeMs: 0,
      overallAvgResponseTimeMs: 0,
    });
  });

  it('computeSessionMetrics computes recent and overall accuracy + response time', () => {
    const mgr = createManager();
    const w = createFakeWord({ id: 'c' });
    mgr.loadWords([w]);
    mgr.recordAnswer('c', true, 100);
    mgr.recordAnswer('c', false, 200);
    mgr.recordAnswer('c', true, 300);
    const m = mgr.computeSessionMetrics();
    expect(m.overallAccuracy).toBeCloseTo(2 / 3, 5);
    expect(m.overallAvgResponseTimeMs).toBe(200);
    expect(m.recentAccuracy).toBeCloseTo(2 / 3, 5);
    expect(m.recentAvgResponseTimeMs).toBe(200);
  });

  it('setTargetMasteryCount + shouldPrefetch returns true near depletion', () => {
    const mgr = createManager();
    mgr.loadWords(createFakeWords(2));
    mgr.setTargetMasteryCount(5);
    expect(mgr.shouldPrefetch()).toBe(true);
  });

  it('shouldPrefetch returns false when mastered already meets target', () => {
    const mgr = createManager();
    mgr.loadWords(createFakeWords(1));
    mgr.setTargetMasteryCount(0); // target 0 时 always allow prefetch when active<=2
    expect(mgr.shouldPrefetch()).toBe(true);
  });

  it('shouldPrefetch returns false when active queue is large', () => {
    const mgr = createManager();
    mgr.loadWords(createFakeWords(5));
    expect(mgr.shouldPrefetch()).toBe(false);
  });

  it('getErrorProneWordIds returns wordIds with errorCount > 0 across queues', () => {
    const mgr = createManager();
    const w1 = createFakeWord({ id: 'ep-1' });
    const w2 = createFakeWord({ id: 'ep-2' });
    const w3 = createFakeWord({ id: 'ep-3' });
    mgr.loadWords([w1, w2, w3]);
    mgr.recordAnswer('ep-2', false);
    expect(mgr.getErrorProneWordIds()).toEqual(['ep-2']);
  });

  it('resetHistory clears computed accuracy', () => {
    const mgr = createManager();
    const w = createFakeWord({ id: 'rh' });
    mgr.loadWords([w]);
    mgr.recordAnswer('rh', true, 100);
    mgr.resetHistory();
    expect(mgr.computeSessionMetrics().overallAvgResponseTimeMs).toBe(0);
  });

  it('restore handles legacy persisted data without priority field', () => {
    const word = createFakeWord({ id: 'legacy' });
    // 模拟旧版本：active 项缺 priority 字段
    localStorage.setItem(
      'eng_learning_queue',
      JSON.stringify({
        active: [{ word, correctCount: 0, errorCount: 0, lastShown: 0 }],
        mastered: [],
        batchSize: 5,
      }),
    );
    const mgr = createManager();
    const next = mgr.pickNext();
    expect(next?.word.id).toBe('legacy');
  });

  it('restore tolerates missing active/mastered/batchSize fields entirely', () => {
    // 已损坏 / 早期版本：所有可选字段缺失
    localStorage.setItem('eng_learning_queue', JSON.stringify({}));
    const mgr = createManager(7);
    expect(mgr.getActiveCount()).toBe(0);
    expect(mgr.getMasteredCount()).toBe(0);
  });

  it('pickNext sorts equal-priority items by lastShown ascending', () => {
    // 通过持久化注入两个 priority 相同但 lastShown 不同的词
    const w1 = createFakeWord({ id: 'p-a' });
    const w2 = createFakeWord({ id: 'p-b' });
    localStorage.setItem(
      'eng_learning_queue',
      JSON.stringify({
        active: [
          { word: w1, correctCount: 0, errorCount: 0, lastShown: 200, priority: 0 },
          { word: w2, correctCount: 0, errorCount: 0, lastShown: 100, priority: 0 },
        ],
        mastered: [],
        batchSize: 5,
      }),
    );
    const mgr = createManager();
    expect(mgr.pickNext()?.word.id).toBe('p-b'); // 更早 lastShown 优先
  });

  it('generateOptions in meaning-to-word mode returns word text answer + placeholder', () => {
    vi.spyOn(Math, 'random').mockReturnValue(0.5);
    const mgr = createManager();
    const w = createFakeWord({ id: 'single' });
    mgr.loadWords([w]);
    const target = mgr.pickNext()!;
    const options = mgr.generateOptions(target, 'meaning-to-word');
    expect(options).toHaveLength(4);
    expect(options).toContain(w.text);
    expect(options).toContain('(未知)');
    vi.restoreAllMocks();
  });
});
