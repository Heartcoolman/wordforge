import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// 新版系统监控页(终端式看板)调用的 adminApi / amasApi 方法全集。
// fetchSlow() 在 Promise.allSettled 中无条件调用 getHealth / monitoringWorkers /
// amasAdvisorCost / checkUpdate / getMetrics / getDatabase;
// 死信抽屉打开后再调用 monitoringDeadLetter / requeue / purge。
vi.mock('@/api/admin', () => ({
  adminApi: {
    monitoringRequests: vi.fn(),
    monitoringLogs: vi.fn(),
    monitoringEvents: vi.fn(),
    getHealth: vi.fn(),
    getDatabase: vi.fn(),
    monitoringWorkers: vi.fn(),
    amasAdvisorCost: vi.fn(),
    checkUpdate: vi.fn(),
    monitoringDeadLetter: vi.fn(),
    monitoringDeadLetterRequeue: vi.fn(),
    monitoringDeadLetterPurge: vi.fn(),
  },
}));
vi.mock('@/api/amas', () => ({
  amasApi: { getMetrics: vi.fn() },
}));
// toast 经 @/components/wf 再导出 uiStore.toast,此 mock 覆盖之
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { amasApi } from '@/api/amas';

const mockAdmin = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockAmas = amasApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

// 各 resolved 值形状对齐 @/types/admin 真实返回类型
const mockRequests = {
  windowSecs: 3600, effectiveSecs: 3600, totalRequests: 1200, total5xx: 1,
  qpsAvg: 0.33, p50Ms: 12, p99Ms: 88, errorRate: 0.0008, availabilityPct: 99.92,
  bandwidthInBps: 2048, series: [],
};
const mockHealth = {
  status: 'healthy' as const, dbSizeBytes: 1048576, uptimeSecs: 3600, version: '1.1.0', errorRate: 0,
  storeProbeOk: true,
  services: {
    amas: { healthy: true },
    sse: { healthy: true, activeConnections: 2, activeDevices: 2, maxConnections: 256 },
    wordbookCenter: { healthy: true },
  },
  outbox: { pending: 0, lagSecs: 0, deadLetter: 0 },
};
const mockDatabase = {
  sizeOnDisk: 1048576, tableCount: 1, tables: ['client_devices'],
  pageSize: 4096, pageCount: 256, walEnabled: true, walSizeBytes: 8192,
};
const mockLogs = { logs: [{ tsMs: 1735689600000, level: 'INFO', target: 'http', message: 'request handled' }] };
const mockEvents = { events: [{ tsMs: 1735689600000, severity: 'warning' as const, title: 'worker 延迟', desc: 'llm_advisor 超时' }] };
const mockWorkers = {
  workers: [{ workerName: 'llm_advisor', lastRunAt: '2025-01-01T00:00:00Z', lastDurationMs: 12, lastOutcome: 'success', lastError: null }],
};
const mockCost = {
  monthYuan: 4.21, monthCapYuan: 10, quotaPct: 42.1, forecastYuan: 6.84, avg7dCostYuan: 0.14,
  monthCalls: 31, acceptedCount: 47, rejectedCount: 6, acceptanceRate: 0.887, usdToCny: 7.3,
};
const mockUpdate = { currentVersion: '1.1.0', latestVersion: '1.1.0', hasUpdate: false, releaseUrl: null, releaseNotes: null };

function primeMocks() {
  mockAdmin.monitoringRequests.mockResolvedValue(mockRequests);
  mockAdmin.monitoringLogs.mockResolvedValue(mockLogs);
  mockAdmin.monitoringEvents.mockResolvedValue(mockEvents);
  mockAdmin.getHealth.mockResolvedValue(mockHealth);
  mockAdmin.getDatabase.mockResolvedValue(mockDatabase);
  mockAdmin.monitoringWorkers.mockResolvedValue(mockWorkers);
  mockAdmin.amasAdvisorCost.mockResolvedValue(mockCost);
  mockAdmin.checkUpdate.mockResolvedValue(mockUpdate);
  mockAdmin.monitoringDeadLetter.mockResolvedValue({ entries: [] });
  mockAdmin.monitoringDeadLetterRequeue.mockResolvedValue({ requeued: true, id: 1 });
  mockAdmin.monitoringDeadLetterPurge.mockResolvedValue({ purged: true, id: 1 });
  mockAmas.getMetrics.mockResolvedValue(null);
}

async function renderPage() {
  const { default: MonitoringPage } = await import('@/pages/MonitoringPage');
  return renderWithProviders(() => <MonitoringPage />);
}

describe('MonitoringPage', () => {
  beforeEach(() => vi.clearAllMocks());

  it('初始展示加载 spinner', async () => {
    // 全部 fetcher 挂起 → onMount 的 refreshAll 永不 resolve,loading 保持 true,渲染 <Loading> 的 spinner
    for (const k of Object.keys(mockAdmin)) {
      mockAdmin[k].mockReturnValue(new Promise(() => {}));
    }
    mockAmas.getMetrics.mockReturnValue(new Promise(() => {}));
    const { container } = await renderPage();
    expect(container.querySelector('.spinner')).toBeTruthy();
  });

  it('加载完成后渲染页头「系统监控」', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('系统监控')).toBeInTheDocument());
  });

  it('渲染服务状态 / Worker 心跳 / 请求指标 / 实时日志 / 告警时间线 区块标题', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('服务状态')).toBeInTheDocument());
    expect(screen.getByText('Worker 心跳')).toBeInTheDocument();
    expect(screen.getByText('请求指标')).toBeInTheDocument();
    expect(screen.getByText('实时日志')).toBeInTheDocument();
    expect(screen.getByText('告警时间线')).toBeInTheDocument();
  });

  it('渲染 SLO/KPI 卡(后端可用性)', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('后端可用性')).toBeInTheDocument());
    expect(screen.getByText('平均 RPS')).toBeInTheDocument();
    expect(screen.getByText('P99 延迟')).toBeInTheDocument();
    expect(screen.getByText('错误率')).toBeInTheDocument();
  });

  it('渲染实时日志行内容(target + message)', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('request handled')).toBeInTheDocument());
    // target 与 message 拆分为独立 span 渲染
    expect(screen.getByText('http')).toBeInTheDocument();
  });

  it('渲染告警事件标题', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('worker 延迟')).toBeInTheDocument());
  });
});
