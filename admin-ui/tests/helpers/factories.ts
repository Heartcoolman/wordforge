import type { User } from '@/types/user';
import type { Word } from '@/types/word';

let idCounter = 0;

export function createFakeUser(overrides?: Partial<User>): User {
  return {
    id: `user-${++idCounter}`,
    email: 'test@example.com',
    username: 'testuser',
    isBanned: false,
    ...overrides,
  };
}

export function createFakeWord(overrides?: Partial<Word>): Word {
  const n = ++idCounter;
  return {
    id: `word-${n}`,
    text: `word${n}`,
    meaning: `含义${n}`,
    difficulty: 3,
    examples: [`Example sentence ${n}`],
    tags: ['test'],
    createdAt: new Date().toISOString(),
    ...overrides,
  };
}

export function createFakeJwt(payload: Record<string, unknown> = {}): string {
  const header = btoa(JSON.stringify({ alg: 'HS256', typ: 'JWT' }));
  const body = btoa(
    JSON.stringify({
      sub: 'user-1',
      exp: Math.floor(Date.now() / 1000) + 3600,
      ...payload,
    }),
  );
  const sig = btoa('fake-signature');
  return `${header}.${body}.${sig}`;
}

export function createFakeWords(count: number): Word[] {
  return Array.from({ length: count }, () => createFakeWord());
}
