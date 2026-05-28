import { describe, it, expect } from 'vitest';
import {
  PARAM_DICT, PARAM_INDEX, ALL_KNOWN_PATHS, TIER_A_META, TIER_A_PATHS, PRESETS,
  getByPath, setByPath, validateField, validateConfig, applyPreset, diffKnown,
} from '@/pages/admin/amas/schema';

describe('schema', () => {
  describe('PARAM_DICT / TIER_A', () => {
    it('all sections have at least one meta', () => {
      for (const [key, list] of Object.entries(PARAM_DICT)) {
        expect(list).not.toHaveLength(0);
        for (const m of list) expect(m.path.startsWith(key + '.') || m.path.startsWith(key + '[')).toBe(true);
      }
    });

    it('TIER_A_META resolves all 11 paths', () => {
      expect(TIER_A_META.length).toBe(TIER_A_PATHS.length);
    });

    it('ALL_KNOWN_PATHS contains every PARAM_INDEX key', () => {
      for (const path of PARAM_INDEX.keys()) {
        expect(ALL_KNOWN_PATHS).toContain(path);
      }
    });

    it('PRESETS has factory_default and tuned', () => {
      const ids = PRESETS.map((p) => p.id);
      expect(ids).toContain('factory_default');
      expect(ids).toContain('tuned_2026_05_15');
    });
  });

  describe('getByPath / setByPath', () => {
    it('returns undefined for unknown path', () => {
      expect(getByPath({}, 'a.b')).toBeUndefined();
    });

    it('reads nested field', () => {
      expect(getByPath({ a: { b: 1 } }, 'a.b')).toBe(1);
    });

    it('reads array element', () => {
      expect(getByPath({ arr: [10, 20, 30] }, 'arr[1]')).toBe(20);
    });

    it('returns undefined when array key is not array', () => {
      expect(getByPath({ arr: 'str' }, 'arr[0]')).toBeUndefined();
    });

    it('returns undefined mid-path when cur is non-object', () => {
      expect(getByPath({ a: 1 }, 'a.b.c')).toBeUndefined();
    });

    it('sets nested field, creating sections', () => {
      const cfg: Record<string, unknown> = {};
      setByPath(cfg, 'a.b.c', 42);
      expect((cfg as any).a.b.c).toBe(42);
    });

    it('sets array element', () => {
      const cfg: Record<string, unknown> = {};
      setByPath(cfg, 'arr[2]', 99);
      expect((cfg as any).arr[2]).toBe(99);
    });

    it('overwrites non-object intermediate', () => {
      const cfg: Record<string, unknown> = { a: 'str' };
      setByPath(cfg, 'a.b', 1);
      expect((cfg as any).a.b).toBe(1);
    });

    it('handles array-of-object intermediate path', () => {
      const cfg: Record<string, unknown> = {};
      setByPath(cfg, 'list[0].k', 'v');
      expect((cfg as any).list[0].k).toBe('v');
    });

    it('replaces existing array entry of non-object type', () => {
      const cfg: Record<string, unknown> = { list: [1] };
      setByPath(cfg, 'list[0].k', 'v');
      expect((cfg as any).list[0].k).toBe('v');
    });
  });

  describe('validateField', () => {
    it('passes valid bool', () => {
      expect(validateField({ path: 'x', label_zh: 'x', type: 'bool', default: false, description_zh: '' }, true)).toBeNull();
    });

    it('rejects non-bool for bool type', () => {
      expect(validateField({ path: 'x', label_zh: 'x', type: 'bool', default: false, description_zh: '' }, 1 as any)).toBe('需为布尔值');
    });

    it('rejects non-finite number', () => {
      expect(validateField({ path: 'x', label_zh: 'x', type: 'number', default: 0, description_zh: '' }, Infinity)).toBe('需为有限数值');
    });

    it('rejects non-integer for integer type', () => {
      expect(validateField({ path: 'x', label_zh: 'x', type: 'integer', default: 0, description_zh: '' }, 1.5)).toBe('需为整数');
    });

    it('rejects below min', () => {
      expect(validateField({ path: 'x', label_zh: 'x', type: 'number', min: 0, default: 0, description_zh: '' }, -1)).toContain('不得小于');
    });

    it('rejects above max', () => {
      expect(validateField({ path: 'x', label_zh: 'x', type: 'number', max: 1, default: 0, description_zh: '' }, 2)).toContain('不得大于');
    });

    it('accepts integer value for integer type within range', () => {
      expect(validateField({ path: 'x', label_zh: 'x', type: 'integer', min: 0, max: 10, default: 0, description_zh: '' }, 5)).toBeNull();
    });
  });

  describe('validateConfig', () => {
    it('returns no error on empty config', () => {
      expect(validateConfig({})).toEqual([]);
    });

    it('detects field range error', () => {
      const errs = validateConfig({ memoryModel: { baseDesiredRetention: 0.5 } });
      expect(errs.some((e) => e.path === 'memoryModel.baseDesiredRetention')).toBe(true);
    });

    it('detects ensemble weights sum mismatch', () => {
      const errs = validateConfig({ ensemble: { baseWeightHeuristic: 0.5, baseWeightIge: 0.5, baseWeightSwd: 0.5 } });
      expect(errs.some((e) => e.message.includes('Heuristic'))).toBe(true);
    });

    it('detects objectiveWeights sum mismatch', () => {
      const errs = validateConfig({
        objectiveWeights: { retention: 0.5, accuracy: 0.5, speed: 0.5, fatigue: 0.5, frustration: 0.5 },
      });
      expect(errs.some((e) => e.message.includes('五项'))).toBe(true);
    });

    it('detects alphaMin >= alphaMax', () => {
      const errs = validateConfig({ memoryModel: { alphaMin: 0.5, alphaMax: 0.5 } });
      expect(errs.some((e) => e.path === 'memoryModel.alphaMax')).toBe(true);
    });

    it('detects retentionMin >= retentionMax', () => {
      const errs = validateConfig({ memoryModel: { retentionMin: 0.9, retentionMax: 0.8 } });
      expect(errs.some((e) => e.path === 'memoryModel.retentionMax')).toBe(true);
    });
  });

  describe('applyPreset / diffKnown', () => {
    it('applyPreset factory uses defaults', () => {
      const out = applyPreset({}, 'factory_default');
      expect(getByPath(out, 'memoryModel.baseDesiredRetention')).toBe(0.92);
    });

    it('applyPreset tuned uses tuned values when defined', () => {
      const out = applyPreset({}, 'tuned_2026_05_15');
      // baseDesiredRetention has tuned 0.849021
      expect(getByPath(out, 'memoryModel.baseDesiredRetention')).toBeCloseTo(0.849021, 5);
    });

    it('diffKnown returns entries that differ', () => {
      const diff = diffKnown({ memoryModel: { baseDesiredRetention: 0.92 } }, { memoryModel: { baseDesiredRetention: 0.85 } });
      expect(diff.some((d) => d.path === 'memoryModel.baseDesiredRetention')).toBe(true);
    });

    it('diffKnown ignores unknown keys', () => {
      const diff = diffKnown({ unknown: 1 }, { unknown: 2 });
      expect(diff.every((d) => d.path !== 'unknown')).toBe(true);
    });
  });
});
