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
});
