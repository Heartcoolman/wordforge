import { describe, it, expect, vi, beforeEach, afterAll } from 'vitest';
import { TEST_BASE_URL as BASE } from '../helpers/constants';

const h = vi.hoisted(() => ({
  getAdminToken: vi.fn((): string | null => 'admin-tok'),
  clearAdminToken: vi.fn(),
}));

vi.mock('@/lib/token', () => ({
  tokenManager: {
    getToken: () => null,
    getAdminToken: h.getAdminToken,
    clearAdminToken: h.clearAdminToken,
    setTokens: vi.fn(),
    clearTokens: vi.fn(),
    needsRefresh: () => false,
    refreshAccessToken: vi.fn(),
    isAuthenticated: () => false,
    setAdminToken: vi.fn(),
  },
}));

import { packsApi } from '@/api/packs';

interface ProgressLike {
  lengthComputable: boolean;
  loaded: number;
  total: number;
}

/** 最小 XHR 替身：只实现 uploadVersion 用到的表面，并把实例暴露给用例手动触发生命周期。 */
class FakeXhr {
  static last: FakeXhr | null = null;
  method = '';
  url = '';
  headers: Record<string, string> = {};
  responseType = '';
  status = 0;
  response: unknown = null;
  sentBody: unknown = undefined;
  upload: { onprogress?: (e: ProgressLike) => void } = {};
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;

  constructor() {
    FakeXhr.last = this;
  }
  open(method: string, url: string) {
    this.method = method;
    this.url = url;
  }
  setRequestHeader(key: string, value: string) {
    this.headers[key] = value;
  }
  send(body: unknown) {
    this.sentBody = body;
  }
}

const realXhr = globalThis.XMLHttpRequest;
globalThis.XMLHttpRequest = FakeXhr as unknown as typeof XMLHttpRequest;
afterAll(() => {
  globalThis.XMLHttpRequest = realXhr;
});

beforeEach(() => {
  vi.clearAllMocks();
  h.getAdminToken.mockReturnValue('admin-tok');
  FakeXhr.last = null;
});

function upload(
  packId = 'my pack',
  query: Parameters<typeof packsApi.uploadVersion>[1] = { version: '1.0.0', channel: 'stable' },
  body: Blob | ArrayBuffer | string = 'payload-bytes',
  onProgress?: (frac: number) => void,
) {
  const promise = packsApi.uploadVersion(packId, query, body, onProgress);
  const xhr = FakeXhr.last!;
  expect(xhr).toBeInstanceOf(FakeXhr);
  return { promise, xhr };
}

describe('packsApi.uploadVersion', () => {
  it('POST 到 buildUrl 解析后的绝对地址，packId 经 encodeURIComponent', () => {
    const { promise, xhr } = upload('my pack');
    expect(xhr.method).toBe('POST');
    expect(xhr.url.startsWith(`${BASE}/api/admin/resource-packs/my%20pack/versions?`)).toBe(true);
    expect(xhr.responseType).toBe('json');
    expect(xhr.sentBody).toBe('payload-bytes');
    xhr.status = 200;
    xhr.response = { data: {} };
    xhr.onload!();
    return promise;
  });

  it('可选 query 为空时不写入 URL，提供时才出现', async () => {
    const bare = upload('p1', { version: '1.0.0', channel: 'beta' });
    const bareParams = new URL(bare.xhr.url).searchParams;
    expect(bareParams.get('version')).toBe('1.0.0');
    expect(bareParams.get('channel')).toBe('beta');
    expect(bareParams.has('minAppVersion')).toBe(false);
    expect(bareParams.has('description')).toBe(false);
    expect(bareParams.has('artifactType')).toBe(false);
    bare.xhr.status = 200;
    bare.xhr.response = { data: {} };
    bare.xhr.onload!();
    await bare.promise;

    const full = upload('p1', {
      version: '2.0.0',
      channel: 'internal',
      minAppVersion: '0.9.0',
      description: 'web 构建',
      artifactType: 'tarball',
    });
    const fullParams = new URL(full.xhr.url).searchParams;
    expect(fullParams.get('minAppVersion')).toBe('0.9.0');
    expect(fullParams.get('description')).toBe('web 构建');
    expect(fullParams.get('artifactType')).toBe('tarball');
    full.xhr.status = 200;
    full.xhr.response = { data: {} };
    full.xhr.onload!();
    await full.promise;
  });

  it('空串的 minAppVersion / description 被当作缺省丢弃', async () => {
    const { promise, xhr } = upload('p1', {
      version: '1.0.0',
      channel: 'stable',
      minAppVersion: '',
      description: '',
    });
    const params = new URL(xhr.url).searchParams;
    expect(params.has('minAppVersion')).toBe(false);
    expect(params.has('description')).toBe(false);
    xhr.status = 201;
    xhr.response = { data: {} };
    xhr.onload!();
    await promise;
  });

  it('有 admin token 时附 Authorization 头', () => {
    const { promise, xhr } = upload();
    expect(xhr.headers.Authorization).toBe('Bearer admin-tok');
    xhr.status = 200;
    xhr.response = { data: {} };
    xhr.onload!();
    return promise;
  });

  it('无 admin token 时不附 Authorization 头', () => {
    h.getAdminToken.mockReturnValue(null);
    const { promise, xhr } = upload();
    expect(xhr.headers.Authorization).toBeUndefined();
    xhr.status = 200;
    xhr.response = { data: {} };
    xhr.onload!();
    return promise;
  });

  it('2xx 解析 response.data', async () => {
    const data = {
      packId: 'p1',
      version: '1.0.0',
      sha256: 'deadbeef',
      signature: 'sig',
      sizeBytes: 1024,
      channel: 'stable' as const,
      artifactType: 'tarball',
    };
    const { promise, xhr } = upload('p1');
    xhr.status = 200;
    xhr.response = { success: true, data };
    xhr.onload!();
    await expect(promise).resolves.toEqual(data);
  });

  it('response 无 data 字段时回退整个 response（299 边界仍算成功）', async () => {
    const bare = { packId: 'p1', version: '1.0.0', sha256: 'x', signature: 's', sizeBytes: 1, channel: 'beta' as const };
    const { promise, xhr } = upload('p1');
    xhr.status = 299;
    xhr.response = bare;
    xhr.onload!();
    await expect(promise).resolves.toEqual(bare);
  });

  it('onProgress 收到 loaded/total 比值；lengthComputable=false 时不回调', async () => {
    const onProgress = vi.fn();
    const { promise, xhr } = upload('p1', { version: '1.0.0', channel: 'stable' }, 'body', onProgress);
    xhr.upload.onprogress!({ lengthComputable: true, loaded: 50, total: 200 });
    xhr.upload.onprogress!({ lengthComputable: true, loaded: 200, total: 200 });
    xhr.upload.onprogress!({ lengthComputable: false, loaded: 10, total: 0 });
    expect(onProgress.mock.calls).toEqual([[0.25], [1]]);
    xhr.status = 200;
    xhr.response = { data: {} };
    xhr.onload!();
    await promise;
  });

  it('未传 onProgress 时进度事件不抛错', async () => {
    const { promise, xhr } = upload('p1');
    expect(() => xhr.upload.onprogress!({ lengthComputable: true, loaded: 1, total: 2 })).not.toThrow();
    xhr.status = 200;
    xhr.response = { data: {} };
    xhr.onload!();
    await promise;
  });

  it('401 清 admin token 并广播 admin:unauthorized，reject 为 HTTP 401', async () => {
    const listener = vi.fn();
    window.addEventListener('admin:unauthorized', listener);
    const { promise, xhr } = upload('p1');
    xhr.status = 401;
    xhr.response = null;
    xhr.onload!();
    await expect(promise).rejects.toThrow('HTTP 401');
    expect(h.clearAdminToken).toHaveBeenCalledTimes(1);
    expect(listener).toHaveBeenCalledTimes(1);
    window.removeEventListener('admin:unauthorized', listener);
  });

  it('非 401 的失败不清 token，优先用服务端 message', async () => {
    const { promise, xhr } = upload('p1');
    xhr.status = 400;
    xhr.response = { message: '版本号已存在' };
    xhr.onload!();
    await expect(promise).rejects.toThrow('版本号已存在');
    expect(h.clearAdminToken).not.toHaveBeenCalled();
  });

  it('失败且无 message 时回退 HTTP <status>', async () => {
    const { promise, xhr } = upload('p1');
    xhr.status = 500;
    xhr.response = {};
    xhr.onload!();
    await expect(promise).rejects.toThrow('HTTP 500');
  });

  it('网络层失败 reject 为「网络错误」', async () => {
    const { promise, xhr } = upload('p1');
    xhr.onerror!();
    await expect(promise).rejects.toThrow('网络错误');
  });
});
