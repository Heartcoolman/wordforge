import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  post: vi.fn(),
  connectSseStream: vi.fn(),
  stopSseStream: vi.fn(),
  getToken: vi.fn(),
  getDeviceId: vi.fn(),
  collectDeviceFingerprint: vi.fn(),
}));

vi.mock('@/api/client', () => ({
  api: {
    post: mocks.post,
  },
  connectSseStream: mocks.connectSseStream,
}));

vi.mock('@/lib/token', () => ({
  tokenManager: {
    getToken: mocks.getToken,
  },
}));

vi.mock('@/lib/device', () => ({
  getDeviceId: mocks.getDeviceId,
  collectDeviceFingerprint: mocks.collectDeviceFingerprint,
}));

import { startTelemetryWorker, stopTelemetryWorker } from '@/workers/telemetry';

describe('telemetry worker', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mocks.post.mockResolvedValue({ id: 'telemetry-1' });
    mocks.connectSseStream.mockReturnValue(mocks.stopSseStream);
    mocks.getToken.mockReturnValue('user-token');
    mocks.getDeviceId.mockReturnValue('device-1');
    mocks.collectDeviceFingerprint.mockReturnValue({
      cpuCores: 8,
      memoryGb: 16,
      screenWidth: 1440,
      screenHeight: 900,
      pixelRatio: 2,
      osName: 'macOS',
      browserName: 'Chrome',
      browserVersion: '123',
      timezone: 'Asia/Shanghai',
      language: 'zh-CN',
      touchSupport: false,
      onlineStatus: true,
    });
  });

  afterEach(() => {
    stopTelemetryWorker();
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it('opens an SSE stream and uploads telemetry on server request', async () => {
    startTelemetryWorker();

    expect(mocks.connectSseStream).toHaveBeenCalledTimes(1);
    const callbacks = mocks.connectSseStream.mock.calls[0][0];

    callbacks.onTelemetryRequest('request-1');
    await Promise.resolve();
    await Promise.resolve();

    expect(mocks.post).toHaveBeenCalledWith('/api/telemetry', expect.objectContaining({
      eventType: 'on_demand',
      requestId: 'request-1',
    }));
  });

  it('closes the SSE stream when telemetry stops', () => {
    startTelemetryWorker();
    stopTelemetryWorker();

    expect(mocks.stopSseStream).toHaveBeenCalledTimes(1);
  });
});
