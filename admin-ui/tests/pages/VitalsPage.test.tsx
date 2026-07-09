import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// VitalsPage 仅依赖 adminApi.health（公开 /health 端点）
const health = vi.fn();
vi.mock('@/api/admin', () => ({ adminApi: { health: (...a: unknown[]) => health(...a) } }));

import VitalsPage from '@/pages/VitalsPage';

const OK_HEALTH = {
  status: 'ok',
  version: '1.2.0-beta.26',
  uptimeSecs: 90_061,
  serverTime: 1_700_000_000,
  dbSizeBytes: 5_000_000,
  availability: { pct: 99.95, effectiveSecs: 86_400, totalRequests: 12_345 },
  services: {
    amas: { healthy: true },
    store: { healthy: true },
    sse: { healthy: true, activeConnections: 3, activeDevices: 2 },
    clock: { healthy: true, driftSecs: 0, thresholdSecs: 60 },
    wordbookCenter: { healthy: true, probeSkipped: false },
  },
};

// 心电示波器：happy-dom 默认无 2D context（getContext 返 null 会提前 return）。
// 这里 stub 一个 no-op 2D context，使 reduced-motion 分支的 draw(0) 真正执行，覆盖绘制代码。
// （setup.ts 已默认把 matchMedia('(prefers-reduced-motion: reduce)') 设为 matches=true）
let ctx: Record<string, ReturnType<typeof vi.fn>>;
// getContext 的重载签名与 vi.spyOn 泛型推导不兼容；仅用于 mockRestore，收窄声明即可
let getCtxSpy: { mockRestore(): void };

beforeEach(() => {
  health.mockReset();
  const m = () => vi.fn();
  ctx = { setTransform: m(), clearRect: m(), beginPath: m(), moveTo: m(), lineTo: m(), stroke: m(), arc: m(), fill: m() };
  getCtxSpy = vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(ctx as unknown as CanvasRenderingContext2D);
});
afterEach(() => {
  getCtxSpy.mockRestore();
  vi.clearAllTimers();
});

describe('VitalsPage', () => {
  it('正常态：渲染脉搏/可用性/子系统，并执行示波器绘制', async () => {
    health.mockResolvedValue(OK_HEALTH);
    renderWithProviders(() => <VitalsPage />);

    await waitFor(() => expect(screen.getAllByText('正常').length).toBeGreaterThan(0));
    expect(screen.getByText('服务体征')).toBeTruthy();
    expect(screen.getByText('可用性')).toBeTruthy();
    expect(screen.getByText('运行时长')).toBeTruthy();
    expect(screen.getByText('sse')).toBeTruthy();          // 子系统行
    expect(screen.getByText('数据源 GET /health · 公开端点')).toBeTruthy();
    // canvas 2D 上下文被取用且 draw(0) 已执行
    expect(getCtxSpy).toHaveBeenCalled();
    await waitFor(() => expect(ctx.clearRect).toHaveBeenCalled());
  });

  it('降级态：某子系统不健康 → 降级', async () => {
    health.mockResolvedValue({ ...OK_HEALTH, services: { ...OK_HEALTH.services, sse: { healthy: false } } });
    renderWithProviders(() => <VitalsPage />);
    await waitFor(() => expect(screen.getAllByText('降级').length).toBeGreaterThan(0));
  });

  it('故障态：health 失败 → 不可达', async () => {
    health.mockRejectedValue(new Error('网络错误'));
    renderWithProviders(() => <VitalsPage />);
    await waitFor(() => expect(screen.getAllByText('不可达').length).toBeGreaterThan(0));
  });

  it('点击刷新再次拉取 /health', async () => {
    health.mockResolvedValue(OK_HEALTH);
    renderWithProviders(() => <VitalsPage />);
    await waitFor(() => expect(screen.getAllByText('正常').length).toBeGreaterThan(0));
    const before = health.mock.calls.length;
    fireEvent.click(screen.getByText('刷新'));
    await waitFor(() => expect(health.mock.calls.length).toBeGreaterThan(before));
  });
});
