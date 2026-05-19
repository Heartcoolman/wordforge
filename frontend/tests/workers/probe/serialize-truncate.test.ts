/**
 * api-bridge.serializeAndTruncate 单测：
 *  - 小对象 → truncated=false，json 是合法 stringify
 *  - 大对象（>256KB）→ truncated=true，json 长度恰为 256KB
 *  - 不可序列化对象（循环引用）→ 返回 "[unserializable: ...]" 字符串
 */

import { describe, it, expect } from 'vitest';
import { serializeAndTruncate } from '@/workers/probe/api-bridge';

const LIMIT = 256 * 1024;

describe('serializeAndTruncate', () => {
  it('小对象不截断', () => {
    const { json, truncated } = serializeAndTruncate({ a: 1, b: 'test' });
    expect(truncated).toBe(false);
    expect(JSON.parse(json)).toEqual({ a: 1, b: 'test' });
  });

  it('超 256KB 截断', () => {
    // 构造一个约 300KB 的对象（50000 * 7 char = 350KB）
    const big = new Array(50000).fill('x'.repeat(7));
    const { json, truncated } = serializeAndTruncate(big);
    expect(truncated).toBe(true);
    expect(json.length).toBe(LIMIT);
  });

  it('恰好 256KB 边界不截断', () => {
    // 构造一个 JSON 字符串长度 = 256KB - 4（留 brackets 与 quotes 余量）
    const filler = 'a'.repeat(LIMIT - 4);
    const { json, truncated } = serializeAndTruncate(filler);
    expect(truncated).toBe(false);
    expect(json.length).toBeLessThanOrEqual(LIMIT);
  });

  it('循环引用返回 unserializable 标记', () => {
    const obj: Record<string, unknown> = { a: 1 };
    obj.self = obj;
    const { json, truncated } = serializeAndTruncate(obj);
    expect(truncated).toBe(false);
    expect(json).toMatch(/unserializable/);
  });

  it('undefined/null 序列化 ok', () => {
    expect(serializeAndTruncate(null).json).toBe('null');
    // JSON.stringify(undefined) 返回 undefined（非字符串）→ catch 路径
    const { json } = serializeAndTruncate(undefined);
    // 由 JSON.stringify(undefined) 返回 undefined，不进 catch，json 是 string 'undefined' 吗？
    // 实际上 JSON.stringify(undefined) 返回 undefined，赋给 string 变量后 String(undefined) = 'undefined'。
    // 但我们的实现 `json = JSON.stringify(value)` 让 json 是 undefined。后续 json.length 会抛 TypeError，
    // 进 catch 返回 unserializable 标记。
    expect(typeof json).toBe('string');
  });
});
