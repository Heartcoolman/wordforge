import { batch, createEffect, createMemo, createSignal, onMount, Show, For } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { HeroCard } from '@/components/ui/HeroCard';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { Input } from '@/components/ui/Input';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { Tabs } from '@/components/ui/Tabs';
import { Table } from '@/components/ui/Table';
import { adminApi, type SseLiveEntry, type RecentlyActiveEntry, type TelemetrySummary, type DataChannelValue } from '@/api/admin';
import type { ClientPlatformAgg, ClientVersionAgg, ClientUpgradePolicy, ListedDevice } from '@/types/admin';
import { uiStore } from '@/stores/ui';

const CHANNEL_LABELS: Record<string, string> = {
  amas: 'AMAS',
  learning: '学习',
  telemetry: '遥测',
};

const CHANNEL_VARIANT: Record<DataChannelValue, 'success' | 'warning' | 'error'> = {
  uploaded: 'success',
  nil: 'warning',
  none: 'error',
};

const STATUS_TEXT: Record<DataChannelValue, string> = {
  uploaded: '已上传',
  nil: '空数据',
  none: '未上传',
};

// 时间戳格式化：兼容 "YYYY-MM-DD HH:MM:SS"（后端老格式）和 ISO 8601；空值/解析失败回退原串
const formatTimestamp = (s: string | null | undefined): string => {
  if (!s) return '';
  const iso = s.includes('T') ? s : s.replace(' ', 'T') + 'Z';
  const d = new Date(iso);
  return isNaN(d.getTime()) ? s : d.toLocaleString();
};

function DataChannelBadge(props: { channel: string; status: DataChannelValue }) {
  return (
    <Badge
      variant={CHANNEL_VARIANT[props.status]}
      size="sm"
      dot
      title={`${CHANNEL_LABELS[props.channel]}: ${STATUS_TEXT[props.status]}`}
    >
      {CHANNEL_LABELS[props.channel]}
    </Badge>
  );
}

export default function DevicesPage() {
  const [sseLive, setSseLive] = createSignal<SseLiveEntry[]>([]);
  const [recentlyActive, setRecentlyActive] = createSignal<RecentlyActiveEntry[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [tab, setTab] = createSignal<'sse' | 'recent' | 'all'>('sse');

  // Ban confirm
  const [banTarget, setBanTarget] = createSignal<{ id: string; action: 'ban' | 'unban' } | null>(null);
  const [banReason, setBanReason] = createSignal('');

  // m027:平台聚合 + 版本分布 + 升级策略(对齐 clients.html 设计图)
  const [platforms, setPlatforms] = createSignal<ClientPlatformAgg[]>([]);
  const [versionRows, setVersionRows] = createSignal<ClientVersionAgg[]>([]);
  const [policies, setPolicies] = createSignal<ClientUpgradePolicy[]>([]);
  const [versionFilter, setVersionFilter] = createSignal<'all' | 'web' | 'ios' | 'android'>('all');

  // m027:升级策略编辑态(per-platform draft;保存后 invalidate)
  const [policyDraft, setPolicyDraft] = createSignal<Record<string, Partial<ClientUpgradePolicy>>>({});
  const [savingPolicy, setSavingPolicy] = createSignal<string | null>(null);

  // m027:强制升级广播 modal
  const [broadcastUpgradeOpen, setBroadcastUpgradeOpen] = createSignal<{ platform: string; below: string; latest: string } | null>(null);
  const [broadcastUpgradeMessage, setBroadcastUpgradeMessage] = createSignal('');
  const [broadcastUpgradeSending, setBroadcastUpgradeSending] = createSignal(false);

  // m027 / m027-G:全部设备 tab — 后端分页 + 搜索 + CSV
  const [allPage, setAllPage] = createSignal(1);
  const [allPerPage] = createSignal(20);
  const [allQ, setAllQ] = createSignal('');
  const [allPlatform, setAllPlatform] = createSignal<'' | 'web' | 'ios' | 'android'>('');
  const [allRows, setAllRows] = createSignal<ListedDevice[]>([]);
  const [allTotal, setAllTotal] = createSignal(0);
  const [allLoading, setAllLoading] = createSignal(false);

  // Telemetry
  const [telemetryDevice, setTelemetryDevice] = createSignal<string | null>(null);
  const [telemetryRecords, setTelemetryRecords] = createSignal<TelemetrySummary[]>([]);
  const [telemetryTotal, setTelemetryTotal] = createSignal(0);
  const [telemetryLoading, setTelemetryLoading] = createSignal(false);
  // 正在请求遥测的 deviceId，避免重复点击"拉取遥测"
  const [requestingTelemetry, setRequestingTelemetry] = createSignal<string | null>(null);
  let telemetryRequestId = 0;

  const loadDevices = async () => {
    try {
      setLoading(true);
      // m027:先拉 SSE 列表(关键路径)→ 再拉平台/版本/策略聚合(非关键,失败仅 warn)。
      // 顺序非并行,避免 happy-dom 在 vi.fn mocks 下 Promise.all 的 microtask 时序
      // 引发"Tab 计数未刷新"测试 flake。
      const data = await adminApi.getClients();
      setSseLive(data.sseLive);
      setRecentlyActive(data.recentlyActive);
      try {
        const dist = await adminApi.getClientsDistribution();
        setPlatforms(dist.platforms);
        setVersionRows(dist.versions);
        setPolicies(dist.policies);
      } catch (e: any) {
        uiStore.toast.warning('设备聚合数据加载失败', e?.message);
      }
    } catch (e: any) {
      uiStore.toast.error('加载设备列表失败', e.message);
    } finally {
      setLoading(false);
    }
  };

  // m027:全部设备 tab — 后端分页查询(独立 loading,避免占用主 tab)
  const loadAllDevices = async () => {
    try {
      setAllLoading(true);
      const data = await adminApi.getClientsPaginated({
        page: allPage(),
        perPage: allPerPage(),
        q: allQ() || undefined,
        platform: allPlatform() || undefined,
      });
      setAllRows(data.data);
      setAllTotal(data.total);
    } catch (e: any) {
      uiStore.toast.error('加载设备分页失败', e.message);
    } finally {
      setAllLoading(false);
    }
  };

  // m027:CSV 导出 — 当前页(对齐设计图"导出 CSV"按钮)。
  // 实现策略:不拉全集(可能 1000+ 行),仅导当前分页结果。需要全集时增大 perPage 再点。
  const exportCsv = () => {
    const rows = allRows();
    if (rows.length === 0) {
      uiStore.toast.info('当前页无数据可导出');
      return;
    }
    const header = ['deviceId', 'platform', 'userId', 'appVersion', 'country', 'firstSeenAt', 'lastSeenAt', 'isBanned'];
    const escape = (v: unknown) => {
      const s = v == null ? '' : String(v);
      return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
    };
    const lines = [
      header.join(','),
      ...rows.map((r) => header.map((k) => escape((r as any)[k])).join(',')),
    ];
    const blob = new Blob([lines.join('\n')], { type: 'text/csv;charset=utf-8;' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = `devices-page-${allPage()}-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click();
    URL.revokeObjectURL(a.href);
  };

  // m027:升级策略保存
  const savePolicy = async (platform: string) => {
    const draft = policyDraft()[platform];
    const original = policies().find((p) => p.platform === platform);
    if (!draft || !original) return;
    setSavingPolicy(platform);
    try {
      await adminApi.putUpgradePolicy(platform, {
        minVersion: draft.minVersion ?? original.minVersion,
        suggestedVersion: draft.suggestedVersion ?? original.suggestedVersion,
        grayscalePct: draft.grayscalePct ?? original.grayscalePct,
        pwaSilentUpdate: draft.pwaSilentUpdate ?? original.pwaSilentUpdate,
      });
      uiStore.toast.success(`${platform} 策略已保存`);
      const dist = await adminApi.getClientsDistribution();
      setPolicies(dist.policies);
      // 清掉本平台 draft
      setPolicyDraft((d) => ({ ...d, [platform]: {} }));
    } catch (e: any) {
      uiStore.toast.error(`${platform} 策略保存失败`, e.message);
    } finally {
      setSavingPolicy(null);
    }
  };

  // m027:强制升级广播
  const sendBroadcastUpgrade = async () => {
    const target = broadcastUpgradeOpen();
    if (!target) return;
    setBroadcastUpgradeSending(true);
    try {
      const result = await adminApi.broadcastUpgrade(target.platform, {
        belowVersion: target.below,
        latestVersion: target.latest,
        message: broadcastUpgradeMessage() || undefined,
      });
      uiStore.toast.success(`已派发 ${target.platform} 强制升级`, `匹配 ${result.matched} 设备 · 触达 ${result.pushedConnections} 个活跃连接`);
      setBroadcastUpgradeOpen(null);
      setBroadcastUpgradeMessage('');
    } catch (e: any) {
      uiStore.toast.error('强制升级广播失败', e.message);
    } finally {
      setBroadcastUpgradeSending(false);
    }
  };

  const handleBan = async () => {
    const t = banTarget();
    if (!t) return;
    const shortId = truncateId(t.id);
    try {
      if (t.action === 'ban') {
        await adminApi.banClient(t.id, banReason() || undefined);
        uiStore.toast.success(`已封禁设备 ${shortId}`);
      } else {
        await adminApi.unbanClient(t.id);
        uiStore.toast.success(`已解封设备 ${shortId}`);
      }
      setBanTarget(null);
      setBanReason('');
      loadDevices();
    } catch (e: any) {
      uiStore.toast.error(`${t.action === 'ban' ? '封禁' : '解封'} ${shortId} 失败`, e.message);
    }
  };

  const requestTelemetry = async (deviceId: string) => {
    const shortId = truncateId(deviceId);
    setRequestingTelemetry(deviceId);
    try {
      await adminApi.requestTelemetry(deviceId);
      uiStore.toast.success(`已向 ${shortId} 发送遥测请求`);
    } catch (e: any) {
      uiStore.toast.error(`向 ${shortId} 请求遥测失败`, e.message);
    } finally {
      setRequestingTelemetry(null);
    }
  };

  const loadTelemetry = async (deviceId: string) => {
    const requestId = ++telemetryRequestId;
    try {
      // 一次性切换设备 + 清空旧数据 + 进入加载态，避免三段渲染闪烁
      batch(() => {
        setTelemetryDevice(deviceId);
        setTelemetryRecords([]);
        setTelemetryTotal(0);
        setTelemetryLoading(true);
      });
      const data = await adminApi.getTelemetry(deviceId);
      if (requestId !== telemetryRequestId) return;
      batch(() => {
        setTelemetryRecords(data.records);
        setTelemetryTotal(data.total);
      });
    } catch (e: any) {
      if (requestId !== telemetryRequestId) return;
      uiStore.toast.error(`加载 ${truncateId(deviceId)} 遥测数据失败`, e.message);
    } finally {
      if (requestId === telemetryRequestId) {
        setTelemetryLoading(false);
      }
    }
  };

  onMount(loadDevices);

  // m027:进入"全部设备"tab 或筛选变化时,拉分页。
  createEffect(() => {
    if (tab() !== 'all') return;
    // 触发 reactive 依赖 allPage / allQ / allPlatform
    allPage();
    allQ();
    allPlatform();
    void loadAllDevices();
  });

  function truncateId(id: string | null | undefined) {
    if (!id) return '';
    return id.length > 12 ? `${id.slice(0, 8)}...${id.slice(-4)}` : id;
  }

  // 平台分布：合并 SSE + Recent 去重后按 platform 聚合(版本字段后端未透出,以平台分布替代)
  const platformDistribution = createMemo<Array<{ platform: string; count: number; share: number }>>(() => {
    const seen = new Set<string>();
    const byPlatform = new Map<string, number>();
    const collect = (entries: Array<{ deviceId: string; platform: string }>) => {
      for (const e of entries) {
        if (seen.has(e.deviceId)) continue;
        seen.add(e.deviceId);
        byPlatform.set(e.platform, (byPlatform.get(e.platform) ?? 0) + 1);
      }
    };
    collect(sseLive());
    collect(recentlyActive());
    const total = Array.from(byPlatform.values()).reduce((a, b) => a + b, 0) || 1;
    return Array.from(byPlatform.entries())
      .map(([platform, count]) => ({ platform, count, share: count / total }))
      .sort((a, b) => b.count - a.count);
  });

  const platformBarColor: Record<string, string> = {
    web: 'var(--accent)',
    ios: 'var(--info)',
    android: 'var(--success)',
  };

  // SSE 表列定义
  const sseColumns = [
    {
      key: 'deviceId',
      title: '设备 ID',
      class: 'whitespace-nowrap',
      render: (r: SseLiveEntry) => (
        <span class="font-mono text-xs tabular-nums" title={r.deviceId}>{truncateId(r.deviceId)}</span>
      ),
    },
    {
      key: 'platform',
      title: '平台',
      render: (r: SseLiveEntry) => (
        <Badge variant="default" size="sm">{r.platform}</Badge>
      ),
    },
    {
      key: 'appVersion',
      title: '版本',
      render: (r: SseLiveEntry) => (
        <span class="font-mono text-xs tabular-nums text-fg-muted">{r.appVersion ?? '—'}</span>
      ),
    },
    {
      key: 'userId',
      title: '用户',
      render: (r: SseLiveEntry) => (
        <span class="font-mono text-xs tabular-nums">{truncateId(r.userId)}</span>
      ),
    },
    {
      key: 'connectedSecs',
      title: '连接时长',
      render: (r: SseLiveEntry) => <span class="tabular-nums">{Math.floor(r.connectedSecs / 60)}m</span>,
    },
    {
      key: 'connectionCount',
      title: '连接数',
      render: (r: SseLiveEntry) => <span class="tabular-nums">{r.connectionCount}</span>,
    },
    {
      key: 'channels',
      title: '数据状态',
      render: (r: SseLiveEntry) => (
        <div class="flex flex-wrap gap-1">
          <DataChannelBadge channel="amas" status={r.dataChannels.amas} />
          <DataChannelBadge channel="learning" status={r.dataChannels.learning} />
          <DataChannelBadge channel="telemetry" status={r.dataChannels.telemetry} />
        </div>
      ),
    },
    {
      key: 'actions',
      title: '操作',
      render: (r: SseLiveEntry) => (
        <div class="flex gap-1 flex-wrap">
          <Show when={r.isBanned} fallback={
            <Button size="xs" variant="danger" disabled={loading()} onClick={() => setBanTarget({ id: r.deviceId, action: 'ban' })}>封禁</Button>
          }>
            <Button size="xs" variant="success" disabled={loading()} onClick={() => setBanTarget({ id: r.deviceId, action: 'unban' })}>解封</Button>
          </Show>
          <Button size="xs" variant="outline" disabled={requestingTelemetry() === r.deviceId} onClick={() => requestTelemetry(r.deviceId)}>拉取遥测</Button>
          <Button size="xs" variant="ghost" onClick={() => loadTelemetry(r.deviceId)}>历史</Button>
        </div>
      ),
    },
  ];

  // Recent 表列定义(无 connection 字段,加状态列)
  const recentColumns = [
    {
      key: 'deviceId',
      title: '设备 ID',
      class: 'whitespace-nowrap',
      render: (r: RecentlyActiveEntry) => (
        <span class="font-mono text-xs tabular-nums" title={r.deviceId}>{truncateId(r.deviceId)}</span>
      ),
    },
    {
      key: 'platform',
      title: '平台',
      render: (r: RecentlyActiveEntry) => <Badge variant="default" size="sm">{r.platform}</Badge>,
    },
    {
      key: 'appVersion',
      title: '版本',
      render: (r: RecentlyActiveEntry) => (
        <span class="font-mono text-xs tabular-nums text-fg-muted">{r.appVersion ?? '—'}</span>
      ),
    },
    {
      key: 'userId',
      title: '用户',
      render: (r: RecentlyActiveEntry) => (
        <span class="font-mono text-xs tabular-nums">{r.userId ? truncateId(r.userId) : '-'}</span>
      ),
    },
    {
      key: 'lastSeenAt',
      title: '最后活跃',
      render: (r: RecentlyActiveEntry) => (
        <span class="text-xs tabular-nums whitespace-nowrap">{formatTimestamp(r.lastSeenAt)}</span>
      ),
    },
    {
      key: 'status',
      title: '状态',
      render: (r: RecentlyActiveEntry) => (
        <Badge variant={r.isBanned ? 'error' : 'success'} size="sm" dot>{r.isBanned ? '已封禁' : '正常'}</Badge>
      ),
    },
    {
      key: 'channels',
      title: '数据状态',
      render: (r: RecentlyActiveEntry) => (
        <div class="flex flex-wrap gap-1">
          <DataChannelBadge channel="amas" status={r.dataChannels.amas} />
          <DataChannelBadge channel="learning" status={r.dataChannels.learning} />
          <DataChannelBadge channel="telemetry" status={r.dataChannels.telemetry} />
        </div>
      ),
    },
    {
      key: 'actions',
      title: '操作',
      render: (r: RecentlyActiveEntry) => (
        <div class="flex gap-1 flex-wrap">
          <Show when={r.isBanned} fallback={
            <Button size="xs" variant="danger" disabled={loading()} onClick={() => setBanTarget({ id: r.deviceId, action: 'ban' })}>封禁</Button>
          }>
            <Button size="xs" variant="success" disabled={loading()} onClick={() => setBanTarget({ id: r.deviceId, action: 'unban' })}>解封</Button>
          </Show>
          <Button size="xs" variant="ghost" onClick={() => loadTelemetry(r.deviceId)}>历史</Button>
        </div>
      ),
    },
  ];

  return (
    <div class="space-y-6">
      <HeroCard
        eyebrow="SSE + Telemetry"
        eyebrowVariant="info"
        title="设备管理"
        desc="所有通过 /api/* 接入后端的 end-user 设备（Web / iOS / Android），含实时 SSE 连接与接入历史统计。"
        meta={[
          { value: sseLive().length, label: '在线连接数' },
          { value: recentlyActive().length, label: '近期接入设备' },
        ]}
        cta={
          <div class="flex gap-2 flex-wrap">
            <Button size="sm" variant="outline" onClick={loadDevices} disabled={loading()}>刷新</Button>
            {/* m027:对齐设计图 page-header 2 action 按钮(全部强制升级 / 新建推送) */}
            <Button
              size="sm"
              variant="warning"
              disabled={policies().every((p) => !p.minVersion)}
              onClick={() => {
                // 找第一个有 minVersion 的 policy 作为入口(用户进 Modal 后可继续选别的平台)
                const target = policies().find((p) => p.minVersion);
                if (target) {
                  setBroadcastUpgradeOpen({
                    platform: target.platform,
                    below: target.minVersion!,
                    latest: target.suggestedVersion || target.minVersion!,
                  });
                }
              }}
            >
              全部强制升级
            </Button>
            <Button size="sm" variant="primary" onClick={() => (window.location.href = '/admin/broadcast')}>
              新建推送
            </Button>
          </div>
        }
      />

      {/* m027:平台 hero 3 卡(Web/iOS/Android × total/active7d/月环比)。
          固定渲染 3 平台,无数据时显示 0 设备占位,对齐设计图视觉骨架 */}
      <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
        <For each={['web', 'ios', 'android'] as const}>
          {(name) => {
            const agg = createMemo(() => platforms().find((p) => p.platform === name));
            const total = createMemo(() => platforms().reduce((a, b) => a + b.total, 0) || 1);
            const share = () => (agg() ? ((agg()!.total / total()) * 100) : 0);
            const pct = () => agg()?.monthOverMonthPct ?? 0;
            const pctClass = () => (pct() >= 0 ? 'text-success-strong' : 'text-error-strong');
            const pctArrow = () => (pct() >= 0 ? '▲' : '▼');
            const label: Record<typeof name, { name: string; sub: string }> = {
              web: { name: 'Web 浏览器', sub: 'PWA · Chrome / Safari / Firefox' },
              ios: { name: 'iOS 原生', sub: 'iPhone / iPad · App Store' },
              android: { name: 'Android 原生', sub: 'Google Play / 应用宝 / APK' },
            };
            return (
              <Card padding="md">
                <div class="flex items-center gap-2">
                  <Badge size="sm" variant={name === 'web' ? 'accent' : name === 'ios' ? 'default' : 'success'}>
                    {name.toUpperCase()}
                  </Badge>
                  <div class="text-xs text-content-tertiary truncate">{label[name].sub}</div>
                </div>
                <div class="text-sm font-semibold mt-2">{label[name].name}</div>
                <div class="mt-2 text-2xl font-bold tabular-nums">
                  {(agg()?.total ?? 0).toLocaleString()}
                  <span class="text-xs text-content-tertiary font-medium ml-1.5">设备 · {share().toFixed(1)}%</span>
                </div>
                <div class="mt-2 flex gap-3 text-xs text-content-secondary tabular-nums">
                  <span>
                    <span class={pctClass()}>{pctArrow()} {Math.abs(pct()).toFixed(1)}%</span> 比上月
                  </span>
                  <span>{(agg()?.active7d ?? 0).toLocaleString()} 在线 (7d)</span>
                </div>
              </Card>
            );
          }}
        </For>
      </div>

      {/* m027:版本分布柱(三平台 × N 档版本)+ 升级策略面板(右侧)。
          外层去 Show,空数据时显示骨架对齐设计图视觉;policies seed 至少 3 行 */}
      <div class="grid grid-cols-1 lg:grid-cols-12 gap-4">
          <Card class="lg:col-span-7" padding="md">
            <div class="flex items-center justify-between mb-3">
              <h3 class="text-sm font-semibold text-content">版本分布 · 各平台</h3>
              <div class="flex gap-1" role="tablist" aria-label="版本分布平台过滤">
                <For each={['all', 'web', 'ios', 'android'] as const}>
                  {(k) => (
                    <Button
                      size="xs"
                      variant={versionFilter() === k ? 'primary' : 'ghost'}
                      onClick={() => setVersionFilter(k)}
                    >
                      {k === 'all' ? '全部' : k.toUpperCase()}
                    </Button>
                  )}
                </For>
              </div>
            </div>
            <For each={(['web', 'ios', 'android'] as const).filter((p) => versionFilter() === 'all' || versionFilter() === p)}>
              {(plt) => {
                const rows = createMemo(() => versionRows().filter((v) => v.platform === plt));
                const max = createMemo(() => rows().reduce((m, r) => Math.max(m, r.count), 0) || 1);
                const total = createMemo(() => rows().reduce((a, b) => a + b.count, 0));
                return (
                  <div class="mt-3 first:mt-0">
                    <h4 class="text-[11px] font-semibold uppercase tracking-wider text-content-secondary mb-2">
                      {plt.toUpperCase()} ({total().toLocaleString()} 设备)
                    </h4>
                    <Show
                      when={rows().length > 0}
                      fallback={<div class="text-xs text-content-tertiary py-1">— 暂无数据 · 等待客户端上报 x-app-version 头</div>}
                    >
                      <For each={rows().slice(0, 6)}>
                        {(r, idx) => (
                          <div class="grid grid-cols-[110px_1fr_60px] items-center gap-2 py-1 text-[12.5px]">
                            <span class="font-mono tabular-nums">
                              <span class={idx() === 0 ? 'text-success-strong font-semibold' : ''}>{r.version === 'unknown' ? '未知' : r.version}</span>
                            </span>
                            <div class="h-2 rounded-full bg-surface-secondary overflow-hidden">
                              <div
                                class="h-full rounded-full transition-[width] duration-base ease-out-expo"
                                style={{
                                  width: `${((r.count / max()) * 100).toFixed(1)}%`,
                                  background: idx() === 0 ? 'var(--accent)' : idx() < 3 ? 'var(--warning)' : 'var(--error)',
                                }}
                              />
                            </div>
                            <span class="font-mono tabular-nums text-right text-content-secondary">{r.count.toLocaleString()}</span>
                          </div>
                        )}
                      </For>
                    </Show>
                  </div>
                );
              }}
            </For>
          </Card>

          {/* m027:强制升级策略面板(右 5/12) */}
          <Card class="lg:col-span-5" padding="md">
            <div class="flex items-center justify-between mb-3">
              <h3 class="text-sm font-semibold text-content">强制升级策略</h3>
              <span class="text-xs text-content-tertiary tabular-nums">每平台独立</span>
            </div>
            <For each={policies()}>
              {(p) => {
                const draftOf = (k: keyof ClientUpgradePolicy) => policyDraft()[p.platform]?.[k];
                const setDraft = (k: keyof ClientUpgradePolicy, v: any) =>
                  setPolicyDraft((d) => ({ ...d, [p.platform]: { ...d[p.platform], [k]: v } }));
                const dirty = () => {
                  const d = policyDraft()[p.platform];
                  return d && Object.keys(d).length > 0;
                };
                return (
                  <div class="border-b border-border-hairline last:border-b-0 py-2.5">
                    <div class="flex items-center justify-between">
                      <Badge size="sm" variant="default">{p.platform.toUpperCase()}</Badge>
                      <Show when={dirty()}>
                        <Button
                          size="xs"
                          variant="primary"
                          disabled={savingPolicy() === p.platform}
                          onClick={() => savePolicy(p.platform)}
                        >
                          {savingPolicy() === p.platform ? '保存中…' : '保存'}
                        </Button>
                      </Show>
                    </div>
                    <div class="grid grid-cols-2 gap-2 mt-2 text-xs">
                      <label class="flex flex-col gap-0.5">
                        <span class="text-content-tertiary">最低支持版本</span>
                        <Input
                          size="sm"
                          placeholder="例如 v0.6.5"
                          value={(draftOf('minVersion') ?? p.minVersion ?? '') as string}
                          onInput={(e) => setDraft('minVersion', e.currentTarget.value || null)}
                        />
                      </label>
                      <label class="flex flex-col gap-0.5">
                        <span class="text-content-tertiary">建议升级版本</span>
                        <Input
                          size="sm"
                          placeholder="例如 v0.7.0"
                          value={(draftOf('suggestedVersion') ?? p.suggestedVersion ?? '') as string}
                          onInput={(e) => setDraft('suggestedVersion', e.currentTarget.value || null)}
                        />
                      </label>
                      <label class="flex flex-col gap-0.5">
                        <span class="text-content-tertiary">灰度 %(0-100)</span>
                        <Input
                          size="sm"
                          type="number"
                          min="0"
                          max="100"
                          value={String(draftOf('grayscalePct') ?? p.grayscalePct)}
                          onInput={(e) => setDraft('grayscalePct', Number(e.currentTarget.value))}
                        />
                      </label>
                      <Show when={p.platform === 'web'} fallback={<div />}>
                        <label class="flex items-center gap-1.5 mt-3.5">
                          <input
                            type="checkbox"
                            class="checkbox"
                            checked={(draftOf('pwaSilentUpdate') ?? p.pwaSilentUpdate) as boolean}
                            onChange={(e) => setDraft('pwaSilentUpdate', e.currentTarget.checked)}
                          />
                          <span class="text-content-secondary">PWA 静默更新</span>
                        </label>
                      </Show>
                    </div>
                    <Show when={p.minVersion}>
                      <Button
                        size="xs"
                        variant="outline"
                        class="mt-2 w-full"
                        onClick={() => {
                          setBroadcastUpgradeOpen({
                            platform: p.platform,
                            below: p.minVersion!,
                            latest: p.suggestedVersion || p.minVersion!,
                          });
                        }}
                      >
                        立即推送 {p.suggestedVersion || p.minVersion} 给老版本设备
                      </Button>
                    </Show>
                  </div>
                );
              }}
            </For>
            <Show when={policies().length === 0}>
              <div class="text-xs text-content-tertiary py-2 text-center">
                — policies 未加载 · 请重启后端让 m024_client_extras migration 跑过
              </div>
            </Show>
          </Card>
        </div>

      {/* Tabs / Telemetry Panel 不受 loading 影响：刷新时 Telemetry 已展开面板不被卸载 */}
      <Tabs
        tabs={[
          { id: 'sse', label: `SSE 实时连接 (${sseLive().length})` },
          { id: 'recent', label: `近期活跃 (${recentlyActive().length})` },
          { id: 'all', label: '全部设备' },
        ]}
        active={tab()}
        onChange={(id) => setTab(id as 'sse' | 'recent' | 'all')}
      />

      {/* 列表/表格区单独 loading 边界 */}
      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        {/* SSE Live */}
        <Show when={tab() === 'sse'}>
          <Show
            when={sseLive().length > 0}
            fallback={<Card variant="outlined" padding="lg"><Empty title="暂无活跃 SSE 连接" description="实时连接数为 0；用户登录后会出现在这里" /></Card>}
          >
            <Table
              columns={sseColumns}
              data={sseLive()}
              aria-label="SSE 实时连接"
              caption="按设备 ID / 平台 / 用户 / 连接时长 / 连接数 / 数据上报状态 列展示活跃 SSE 连接"
            />
          </Show>
        </Show>

        {/* Recently Active */}
        <Show when={tab() === 'recent'}>
          <Show
            when={recentlyActive().length > 0}
            fallback={<Card variant="outlined" padding="lg"><Empty title="暂无近期活跃设备" description="最近 24h 内没有设备上报过活动" /></Card>}
          >
            <Table
              columns={recentColumns}
              data={recentlyActive()}
              aria-label="近期活跃设备"
              caption="按设备 ID / 平台 / 用户 / 最后活跃 / 封禁状态 / 数据上报状态 列展示最近接入设备"
            />
          </Show>
        </Show>

        {/* m027 / m027-G:全部设备 — 后端分页 + 搜索 + 平台 filter + CSV 导出 + country 列 */}
        <Show when={tab() === 'all'}>
          <Card padding="md">
            <div class="flex gap-2 mb-3 items-center">
              <Input
                size="sm"
                placeholder="搜索设备 ID / 用户 ID"
                value={allQ()}
                onInput={(e) => {
                  batch(() => {
                    setAllQ(e.currentTarget.value);
                    setAllPage(1);
                  });
                }}
                class="max-w-[260px]"
              />
              <select
                class="select select-sm"
                value={allPlatform()}
                onChange={(e) => {
                  batch(() => {
                    setAllPlatform(e.currentTarget.value as any);
                    setAllPage(1);
                  });
                }}
              >
                <option value="">全部平台</option>
                <option value="web">Web</option>
                <option value="ios">iOS</option>
                <option value="android">Android</option>
              </select>
              <div class="ml-auto flex gap-2">
                <Button size="sm" variant="outline" onClick={exportCsv} disabled={allRows().length === 0}>导出 CSV</Button>
              </div>
            </div>
            <Show when={!allLoading()} fallback={<div class="flex justify-center py-8"><Spinner /></div>}>
              <Show
                when={allRows().length > 0}
                fallback={<Empty title="暂无设备记录" description="筛选条件下没有命中数据" />}
              >
                <Table
                  columns={[
                    {
                      key: 'deviceId',
                      title: '设备 ID',
                      render: (r: ListedDevice) => (
                        <span class="font-mono text-xs tabular-nums" title={r.deviceId}>{truncateId(r.deviceId)}</span>
                      ),
                    },
                    {
                      key: 'platform',
                      title: '平台',
                      render: (r: ListedDevice) => <Badge size="sm" variant="default">{r.platform}</Badge>,
                    },
                    {
                      key: 'appVersion',
                      title: '版本',
                      render: (r: ListedDevice) => (
                        <span class="font-mono text-xs tabular-nums">{r.appVersion ?? '—'}</span>
                      ),
                    },
                    {
                      key: 'country',
                      title: '国家',
                      render: (r: ListedDevice) => (
                        <span class="text-xs tabular-nums">{r.country ?? '—'}</span>
                      ),
                    },
                    {
                      key: 'userId',
                      title: '用户',
                      render: (r: ListedDevice) => (
                        <span class="font-mono text-xs tabular-nums">{r.userId ? truncateId(r.userId) : '—'}</span>
                      ),
                    },
                    {
                      key: 'lastSeenAt',
                      title: '最后活跃',
                      render: (r: ListedDevice) => (
                        <span class="text-xs tabular-nums whitespace-nowrap">{formatTimestamp(r.lastSeenAt)}</span>
                      ),
                    },
                    {
                      key: 'status',
                      title: '状态',
                      render: (r: ListedDevice) => (
                        <Badge size="sm" variant={r.isBanned ? 'error' : 'success'} dot>
                          {r.isBanned ? '已封禁' : '正常'}
                        </Badge>
                      ),
                    },
                  ]}
                  data={allRows()}
                  aria-label="全部设备分页"
                />
                <div class="flex items-center justify-between mt-3 text-xs">
                  <span class="text-content-tertiary tabular-nums">
                    显示 {(allPage() - 1) * allPerPage() + 1}–{Math.min(allPage() * allPerPage(), allTotal())} 共 {allTotal()} 台
                  </span>
                  <div class="flex gap-2 items-center">
                    <Button size="xs" variant="ghost" disabled={allPage() <= 1} onClick={() => setAllPage(allPage() - 1)}>‹</Button>
                    <span class="tabular-nums">{allPage()} / {Math.max(1, Math.ceil(allTotal() / allPerPage()))}</span>
                    <Button size="xs" variant="ghost"
                      disabled={allPage() >= Math.ceil(allTotal() / allPerPage())}
                      onClick={() => setAllPage(allPage() + 1)}
                    >›</Button>
                  </div>
                </div>
              </Show>
            </Show>
          </Card>
        </Show>
      </Show>

      {/* Telemetry Panel — 不被刷新 loading 卸载，避免已展开的遥测面板因主列表刷新而消失 */}
      <Show when={telemetryDevice()}>
        <Card class="mt-4">
          <div class="flex items-center justify-between mb-3">
            <h3 class="text-sm font-semibold">遥测记录 — <span class="font-mono">{truncateId(telemetryDevice()!)}</span> (共 {telemetryTotal()} 条)</h3>
            <Button size="xs" variant="ghost" onClick={() => setTelemetryDevice(null)}>关闭</Button>
          </div>
          <Show when={!telemetryLoading()} fallback={<div class="flex justify-center py-4"><Spinner /></div>}>
            <Show when={telemetryRecords().length > 0} fallback={<Empty title="暂无遥测记录" description="该设备尚未上传遥测数据" />}>
              <div class="space-y-2 max-h-80 overflow-y-auto">
                <For each={telemetryRecords()}>
                    {(record) => (
                      <Card variant="outlined" padding="sm" class="text-xs space-y-2">
                        <div class="flex justify-between items-center text-content-secondary gap-2 min-w-0">
                          <Badge variant="default" size="sm">{record.eventType}</Badge>
                          <span class="tabular-nums whitespace-nowrap">{formatTimestamp(record.serverTs)}</span>
                        </div>
                        {/* 设备信息 */}
                        <Show when={record.deviceProfile.osName || record.deviceProfile.browserName}>
                          <div class="bg-surface-secondary rounded p-1.5 space-y-0.5">
                            <div class="font-semibold text-info mb-1 flex items-center gap-1.5">
                              <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                              </svg>
                              设备信息
                            </div>
                            <Show when={record.deviceProfile.osName}>
                              <div class="flex gap-2"><span class="text-content-tertiary w-12">系统</span><span>{record.deviceProfile.osName}</span></div>
                            </Show>
                            <Show when={record.deviceProfile.browserName}>
                              <div class="flex gap-2"><span class="text-content-tertiary w-12">浏览器</span><span>{record.deviceProfile.browserName} {record.deviceProfile.browserVersion}</span></div>
                            </Show>
                            <Show when={record.deviceProfile.cpuCores !== null}>
                              <div class="flex gap-2"><span class="text-content-tertiary w-12">CPU</span><span>{record.deviceProfile.cpuCores} 核{record.deviceProfile.memoryGb ? ` / ${record.deviceProfile.memoryGb}GB` : ''}</span></div>
                            </Show>
                            <Show when={record.deviceProfile.screenWidth !== null}>
                              <div class="flex gap-2"><span class="text-content-tertiary w-12">分辨率</span><span>{record.deviceProfile.screenWidth}×{record.deviceProfile.screenHeight} @{record.deviceProfile.pixelRatio}x</span></div>
                            </Show>
                            <Show when={record.deviceProfile.timezone}>
                              <div class="flex gap-2"><span class="text-content-tertiary w-12">时区</span><span>{record.deviceProfile.timezone} / {record.deviceProfile.language}</span></div>
                            </Show>
                          </div>
                        </Show>
                        {/* 会话统计 */}
                        <div class="bg-surface-secondary rounded p-1.5 space-y-0.5">
                          <div class="font-semibold text-accent mb-1 flex items-center gap-1.5">
                            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                              <path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                            </svg>
                            会话统计
                          </div>
                          <div class="flex gap-4 flex-wrap">
                            <span><span class="text-content-tertiary">时长 </span>{record.sessionStats.sessionDurationSecs}s</span>
                            <span><span class="text-content-tertiary">操作/分 </span>{record.sessionStats.actionsPerMin.toFixed(1)}</span>
                            <span><span class="text-content-tertiary">错误 </span>{record.sessionStats.errorCount}</span>
                            <span><span class="text-content-tertiary">响应 </span>{record.sessionStats.avgResponseTimeMs.toFixed(0)}ms</span>
                          </div>
                        </div>
                        {/* 行为摘要 */}
                        <Show when={record.behaviorSummary.currentRoute !== null}>
                          <div class="bg-surface-secondary rounded p-1.5 space-y-0.5">
                            <div class="font-semibold text-success mb-1 flex items-center gap-1.5">
                              <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M15 15l-2 5L9 9l11 4-5 2zm0 0l5 5M7.188 2.239l.777 2.897M5.136 7.965l-2.898-.777M13.95 4.05l-2.122 2.122m-5.657 5.656l-2.12 2.122" />
                              </svg>
                              行为摘要
                            </div>
                            <Show when={record.behaviorSummary.currentRoute}>
                              <div class="flex gap-2"><span class="text-content-tertiary w-12">路由</span><span class="font-mono">{record.behaviorSummary.currentRoute}</span></div>
                            </Show>
                            <div class="flex gap-4 flex-wrap">
                              <Show when={record.behaviorSummary.clickCount !== null}>
                                <span><span class="text-content-tertiary">点击 </span>{record.behaviorSummary.clickCount}</span>
                              </Show>
                              <Show when={record.behaviorSummary.scrollDepthPct !== null}>
                                <span><span class="text-content-tertiary">滚动 </span>{record.behaviorSummary.scrollDepthPct!.toFixed(0)}%</span>
                              </Show>
                              <Show when={record.behaviorSummary.routeChanges !== null}>
                                <span><span class="text-content-tertiary">跳转 </span>{record.behaviorSummary.routeChanges}</span>
                              </Show>
                              <Show when={record.behaviorSummary.visibilityChanges !== null}>
                                <span><span class="text-content-tertiary">焦点变更 </span>{record.behaviorSummary.visibilityChanges}</span>
                              </Show>
                            </div>
                          </div>
                        </Show>
                        {/* 功能使用 */}
                        <Show when={Object.keys(record.featureUsage).length > 0}>
                          <div class="bg-surface-secondary rounded p-1.5">
                            <div class="font-semibold text-warning mb-1 flex items-center gap-1.5">
                              <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M11 4a2 2 0 114 0v1a1 1 0 001 1h3a1 1 0 011 1v3a1 1 0 01-1 1h-1a2 2 0 100 4h1a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1v-1a2 2 0 10-4 0v1a1 1 0 01-1 1H7a1 1 0 01-1-1v-3a1 1 0 00-1-1H4a2 2 0 110-4h1a1 1 0 001-1V7a1 1 0 011-1h3a1 1 0 001-1V4z" />
                              </svg>
                              功能使用
                            </div>
                            <div class="flex gap-3 flex-wrap">
                              <For each={Object.entries(record.featureUsage)}>
                                {([k, v]) => <span class="tabular-nums"><span class="text-content-tertiary">{k} </span>{v}</span>}
                              </For>
                            </div>
                          </div>
                        </Show>
                      </Card>
                    )}
                  </For>
                </div>
              </Show>
            </Show>
          </Card>
        </Show>

      {/* Ban Confirm Dialog */}
      <Show when={banTarget()}>
        {(target) => (
          <ConfirmDialog
            open={true}
            title={target().action === 'ban' ? '确认封禁设备' : '确认解封设备'}
            message={<>设备 ID: <span class="font-mono">{truncateId(target().id)}</span></>}
            confirmText={target().action === 'ban' ? '确认封禁' : '确认解封'}
            variant={target().action === 'ban' ? 'danger' : 'success'}
            onConfirm={handleBan}
            onCancel={() => { setBanTarget(null); setBanReason(''); }}
          >
            <Show when={target().action === 'ban'}>
              <Input
                type="text"
                placeholder="封禁原因（可选）"
                value={banReason()}
                onInput={(e) => setBanReason(e.currentTarget.value)}
                maxlength={500}
              />
            </Show>
          </ConfirmDialog>
        )}
      </Show>

      {/* m027:强制升级广播 Confirm Dialog */}
      <Show when={broadcastUpgradeOpen()}>
        {(target) => (
          <ConfirmDialog
            open={true}
            title="确认推送强制升级"
            message={
              <span>
                平台 <strong>{target().platform.toUpperCase()}</strong>:
                所有低于 <span class="font-mono">{target().below}</span> 的设备将收到 SSE 升级提示,目标版本 <span class="font-mono">{target().latest}</span>。
              </span>
            }
            confirmText={broadcastUpgradeSending() ? '推送中…' : '确认推送'}
            variant="warning"
            onConfirm={sendBroadcastUpgrade}
            onCancel={() => { setBroadcastUpgradeOpen(null); setBroadcastUpgradeMessage(''); }}
          >
            <Input
              type="text"
              placeholder="附加消息(可选, 显示在客户端横幅)"
              value={broadcastUpgradeMessage()}
              onInput={(e) => setBroadcastUpgradeMessage(e.currentTarget.value)}
              maxlength={300}
            />
          </ConfirmDialog>
        )}
      </Show>
    </div>
  );
}
