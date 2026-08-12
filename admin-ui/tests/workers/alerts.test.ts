import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { SseCallbacks } from '@/api/http';

const h = vi.hoisted(() => ({
  connectSseStream: vi.fn(),
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

vi.mock('@/api/http', () => ({ connectSseStream: h.connectSseStream }));
vi.mock('@/stores/ui', () => ({ uiStore: { toast: h.toast } }));

// alerts.ts 持有模块级 stopSseStream，必须每个用例重新 import 拿到干净状态
async function loadAlerts() {
  vi.resetModules();
  return import('@/workers/alerts');
}

function lastCallbacks(): SseCallbacks {
  return h.connectSseStream.mock.calls.at(-1)![0] as SseCallbacks;
}

describe('startOpsAlerts / stopOpsAlerts', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.connectSseStream.mockReturnValue(vi.fn());
  });

  it('只订阅一次，运行中重复调用是 no-op', async () => {
    const { startOpsAlerts } = await loadAlerts();
    startOpsAlerts();
    startOpsAlerts();
    startOpsAlerts();
    expect(h.connectSseStream).toHaveBeenCalledTimes(1);
  });

  it('注册了 incident / worker_missed / llm_budget_exceeded 三个回调', async () => {
    const { startOpsAlerts } = await loadAlerts();
    startOpsAlerts();
    const cbs = lastCallbacks();
    expect(typeof cbs.onIncident).toBe('function');
    expect(typeof cbs.onWorkerMissed).toBe('function');
    expect(typeof cbs.onLlmBudgetExceeded).toBe('function');
  });

  it('onIncident 用 error toast，错误率换算成两位小数百分比', async () => {
    const { startOpsAlerts } = await loadAlerts();
    startOpsAlerts();
    lastCallbacks().onIncident!({ errorRate: 0.05236, windowSecs: 300 });
    expect(h.toast.error).toHaveBeenCalledWith(
      '5xx 错误率告警',
      '近 300s 窗口错误率 5.24%，请检查监控面板',
    );
  });

  it('onWorkerMissed 用 warning toast，带 worker 名与连续缺席次数', async () => {
    const { startOpsAlerts } = await loadAlerts();
    startOpsAlerts();
    lastCallbacks().onWorkerMissed!({ workerName: 'amas_tick', missCount: 3 });
    expect(h.toast.warning).toHaveBeenCalledWith(
      '后台任务调度异常',
      'worker「amas_tick」已连续 3 个周期未上报',
    );
  });

  it('onLlmBudgetExceeded 用 warning toast，金额固定两位小数', async () => {
    const { startOpsAlerts } = await loadAlerts();
    startOpsAlerts();
    lastCallbacks().onLlmBudgetExceeded!({ spentYuan: 123.456, capYuan: 100, resumeMonth: '2026-09' });
    expect(h.toast.warning).toHaveBeenCalledWith(
      'LLM 月度预算超限',
      '本月已花费 ¥123.46（上限 ¥100.00），advisor 已停跑，2026-09 恢复',
    );
  });

  it('stopOpsAlerts 调用退订函数，之后可重新启动', async () => {
    const unsub = vi.fn();
    h.connectSseStream.mockReturnValue(unsub);
    const { startOpsAlerts, stopOpsAlerts } = await loadAlerts();
    startOpsAlerts();
    stopOpsAlerts();
    expect(unsub).toHaveBeenCalledTimes(1);

    startOpsAlerts();
    expect(h.connectSseStream).toHaveBeenCalledTimes(2);
  });

  it('重复 stop 不会二次退订；未启动就 stop 也不抛错', async () => {
    const unsub = vi.fn();
    h.connectSseStream.mockReturnValue(unsub);
    const { startOpsAlerts, stopOpsAlerts } = await loadAlerts();
    expect(() => stopOpsAlerts()).not.toThrow();
    expect(unsub).not.toHaveBeenCalled();

    startOpsAlerts();
    stopOpsAlerts();
    stopOpsAlerts();
    expect(unsub).toHaveBeenCalledTimes(1);
  });
});
